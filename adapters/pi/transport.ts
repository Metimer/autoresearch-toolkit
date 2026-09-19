import { spawn, type ChildProcess } from "node:child_process";
import { isAbsolute } from "node:path";

export type Envelope = Record<string, unknown> & {
  schema_version: 1;
  command: string;
  ok: boolean;
};
export function object(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown> : {};
}

/** One owned CLI process. Rust alone owns and cleans up experiment descendants. */
export class Transport {
  private child?: ChildProcess;
  private completion?: Promise<Envelope>;
  readonly binary: string;
  readonly root: string;
  constructor(binary: string, root: string) {
    if (!isAbsolute(binary) || !isAbsolute(root)) {
      throw new Error("The engine binary and pilot root must be absolute paths.");
    }
    this.binary = binary;
    this.root = root;
  }

  run(args: string[], signal?: AbortSignal): Promise<Envelope> {
    if (this.child) return Promise.reject(new Error("An engine operation is still running."));
    if (signal?.aborted) return Promise.reject(new Error("Cancelled before launch."));
    const child = spawn(this.binary, [...args, "--json"], {
      cwd: this.root, shell: false, stdio: ["ignore", "pipe", "pipe"],
    });
    this.child = child;
    const abort = () => { if (child.exitCode === null && child.signalCode === null) child.kill("SIGTERM"); };
    signal?.addEventListener("abort", abort, { once: true });
    if (signal?.aborted) abort();
    const completion = new Promise<Envelope>((resolve, reject) => {
      const output: Buffer[] = [];
      let bytes = 0;
      let failure: Error | undefined;
      const collect = (chunk: Buffer, stdout: boolean) => {
        bytes += chunk.length;
        if (bytes > 16 * 1024 * 1024) {
          failure ??= new Error("Engine response exceeds 16 MiB; inspect the session before retrying.");
          abort();
        } else if (stdout) output.push(chunk);
      };
      child.stdout!.on("data", (chunk: Buffer) => collect(chunk, true));
      child.stderr!.on("data", (chunk: Buffer) => collect(chunk, false));
      child.on("error", (error) => { failure = error; });
      child.on("close", (code, killedBy) => {
        signal?.removeEventListener("abort", abort);
        this.child = undefined;
        this.completion = undefined;
        if (failure) return reject(failure);
        if (killedBy) return reject(new Error(`Engine exited on ${killedBy}; inspect recovery before retrying.`));
        try {
          const response = object(JSON.parse(Buffer.concat(output).toString("utf8")));
          if (response.schema_version !== 1 || typeof response.ok !== "boolean"
            || (response.ok && response.command !== args[0])
            || response.ok !== (code === 0)) {
            throw new Error("Incompatible engine JSON envelope or exit status.");
          }
          if (!response.ok) {
            const error = object(response.error);
            throw new Error(`Engine ${String(error.code ?? "error")}: ${String(error.message ?? "operation failed")}`);
          }
          if (args[0] === "baseline" || args[0] === "evaluate" || args[0] === "report") {
            const report = object(object(response.evaluation).report);
            if (!["qualified", "kept", "discarded", "inconclusive", "failed", "cancelled"].includes(String(report.decision))) {
              throw new Error("Missing or unsupported engine verdict.");
            }
          }
          resolve(response as Envelope);
        } catch (error) { reject(error); }
      });
    });
    this.completion = completion;
    return completion;
  }

  async cancel(): Promise<void> {
    const child = this.child;
    const completion = this.completion;
    if (!child || !completion) return;
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGTERM");
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      await Promise.race([
        completion.then(() => undefined, () => undefined),
        new Promise<never>((_, reject) => {
          timer = setTimeout(() => reject(new Error("Engine cleanup is still pending; inspect recovery. No new operation is allowed.")), 5000);
        }),
      ]);
    } finally { clearTimeout(timer); }
  }
}

export function describe(result: Envelope): string {
  const evaluation = object(result.evaluation);
  const report = object(evaluation.report);
  if (typeof report.decision === "string") {
    return `${result.command}: ${report.decision}\nReason: ${String(report.reason)}\nEvidence SHA-256: ${String(evaluation.sha256)}`;
  }
  // Tool details retain the complete envelope; the model-facing view is bounded.
  const text = JSON.stringify(result, null, 2);
  return text.length <= 12000 ? text : `${text.slice(0, 12000)}\n[View truncated; full engine envelope is in tool details.]`;
}
