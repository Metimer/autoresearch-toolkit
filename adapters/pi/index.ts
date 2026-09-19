import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { Transport, describe, object } from "./transport.ts";

const actions = ["status", "history", "report", "workspace", "prepare-candidate", "seal", "baseline", "evaluate", "resume", "stop", "export-candidate"] as const;
const schema = Type.Object({
  action: Type.Union(actions.map((action) => Type.Literal(action))),
  operation_id: Type.Optional(Type.String({ minLength: 1, maxLength: 128 })),
  candidate: Type.Optional(Type.String({ minLength: 1, maxLength: 128 })),
  hypothesis: Type.Optional(Type.String({ minLength: 1, maxLength: 4096 })),
  local_changes: Type.Optional(Type.Union([Type.Literal("exclude"), Type.Literal("include")])),
  output: Type.Optional(Type.String({ minLength: 1 })),
  evaluation: Type.Optional(Type.String({ minLength: 1 })),
}, { additionalProperties: false });

export default function autoresearch(pi: ExtensionAPI) {
  for (const [name, description] of [
    ["autoresearch-engine", "Absolute path to the trusted Rust autoresearch binary"],
    ["autoresearch-root", "Absolute path to the existing engine pilot directory"],
    ["autoresearch-session", "Existing engine session ID; initialize it explicitly with the CLI"],
  ]) pi.registerFlag(name, { type: "string", description });

  let transport: Transport | undefined;
  let session = "";
  let busy = false;
  let transitioning = false;
  let generation = 0;
  let compatible = false;
  function connection() {
    if (!transport) {
      const flags = ["autoresearch-engine", "autoresearch-root", "autoresearch-session"].map((name) => pi.getFlag(name));
      if (flags.some((value) => typeof value !== "string" || !value)) {
        throw new Error("Set --autoresearch-engine, --autoresearch-root and --autoresearch-session explicitly.");
      }
      session = flags[2] as string;
      if (!/^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/.test(session)) throw new Error("Invalid engine session ID.");
      transport = new Transport(flags[0] as string, flags[1] as string);
    }
    return transport;
  }
  async function negotiate(signal?: AbortSignal) {
    const client = connection();
    if (!compatible) {
      const doctor = await client.run(["doctor"], signal);
      const capabilities = object(doctor.capabilities);
      if (doctor.version !== "0.2.0-dev" || capabilities.run_experiments !== true || capabilities.inspect_evaluations !== true || capabilities.pi_adapter !== true || capabilities.hook_protocol_version !== 1) {
        throw new Error("This adapter requires the 0.2.0-dev engine with execution and inspection capabilities.");
      }
      compatible = true;
    }
    return client;
  }
  const status = (ctx: ExtensionContext, text?: string) => {
    // Pi can invalidate the old UI context during session replacement. A view
    // update must not interrupt cleanup or hide a recorded engine result.
    try { if (ctx.hasUI) ctx.ui.setStatus("autoresearch-engine", text); } catch { /* stale UI */ }
  };

  pi.registerTool({
    name: "autoresearch_engine", label: "Autoresearch engine", parameters: schema,
    description: "Operate the explicitly bound Rust session. Rust determines all verdicts. Mutations require an explicit stable operation_id; repeat it only for the same request. Initialize sessions with the CLI. Resume never starts an experiment. No automatic loop.",
    promptSnippet: "Inspect or run an explicitly authorized, bounded Rust experiment",
    promptGuidelines: ["Use autoresearch_engine only within the user's authorized session, scope and budget. Never infer permission from next_action. Read the engine verdict, not just tool success. Edit only the candidate path returned by prepare-candidate."],
    executionMode: "sequential",
    async execute(_id, params, signal, onUpdate, ctx) {
      if (busy || transitioning) throw new Error("The adapter is busy or changing Pi sessions.");
      const required: Record<string, string[]> = {
        status: [], history: [], report: [], workspace: ["local_changes"],
        "prepare-candidate": ["candidate", "hypothesis"], seal: ["candidate"],
        baseline: [], evaluate: ["candidate"], resume: [], stop: [], "export-candidate": ["candidate", "output"],
      };
      if (!actions.includes(params.action)) throw new Error("Unsupported engine action.");
      const fields = [...required[params.action]];
      if (!["status", "history", "report"].includes(params.action)) fields.push("operation_id");
      for (const field of fields) if (!params[field as keyof typeof params]) throw new Error(`Missing ${field}.`);
      const permitted = new Set(["action", ...fields, ...(params.action === "report" ? ["evaluation"] : [])]);
      for (const key of Object.keys(params)) if (!permitted.has(key)) throw new Error(`Unexpected ${key} for ${params.action}.`);
      busy = true;
      const startedGeneration = generation;
      status(ctx, `Autoresearch: ${params.action}`);
      try {
        const client = await negotiate(signal);
        if (transitioning || startedGeneration !== generation) throw new Error("Pi operation was cancelled before launch.");
        const args = [params.action, "--root", client.root, "--session", session];
        for (const field of [...fields, ...(params.action === "report" ? ["evaluation"] : [])]) {
          const value = params[field as keyof typeof params];
          if (value !== undefined) args.push(`--${field.replaceAll("_", "-")}`, value);
        }
        onUpdate?.({ content: [{ type: "text", text: `Engine ${params.action} is running. Await its recorded result.` }], details: undefined });
        if (startedGeneration !== generation) throw new Error("Pi operation was cancelled before launch.");
        const result = await client.run(args, signal);
        const text = describe(result);
        status(ctx, text.split("\n")[0]);
        return { content: [{ type: "text", text }], details: result };
      } catch (error) {
        status(ctx, "Autoresearch: inspect session after error");
        throw error;
      } finally { busy = false; }
    },
  });

  pi.registerCommand("autoresearch-stop", {
    description: "Cancel the current owned engine call and await process cleanup",
    handler: async (_args, ctx) => {
      generation++;
      await transport?.cancel();
      status(ctx, "Autoresearch: current call finished; inspect session status");
    },
  });
  const leave = async (_event: unknown, ctx: ExtensionContext) => {
    generation++;
    transitioning = true;
    try {
      await transport?.cancel();
      status(ctx);
      // Tree navigation, or another extension cancelling a transition, need not
      // emit session_start. The generation still invalidates pending launches.
      transitioning = false;
    }
    catch (error) {
      if (ctx.hasUI) ctx.ui.notify(String(error), "error");
      return { cancel: true };
    }
  };
  pi.on("session_before_switch", leave);
  pi.on("session_before_fork", leave);
  pi.on("session_before_tree", leave);
  pi.on("session_shutdown", async (_event, ctx) => {
    generation++;
    transitioning = true;
    await transport?.cancel();
    status(ctx);
  });
  pi.on("session_start", async (_event, ctx) => {
    transitioning = false;
    status(ctx);
  });
}
