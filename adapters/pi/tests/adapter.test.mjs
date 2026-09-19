import assert from "node:assert/strict";
import { test } from "node:test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { discoverAndLoadExtensions, ExtensionRunner, SessionManager } from "@earendil-works/pi-coding-agent";

const repository = fileURLToPath(new URL("../../../", import.meta.url));
const binary = resolve(repository, "target/debug/autoresearch");
const extension = fileURLToPath(new URL("../index.ts", import.meta.url));
const template = resolve(repository, "examples/session.json");

function fixture(t, blockedHook = false) {
  assert.ok(existsSync(binary), "Run cargo build --locked before the adapter tests.");
  const temp = mkdtempSync(join(tmpdir(), "autoresearch pi é "));
  t.after(() => rmSync(temp, { recursive: true, force: true }));
  const root = join(temp, "pilote é");
  const source = join(temp, "source é");
  mkdirSync(root); mkdirSync(source); mkdirSync(join(source, "src"));
  writeFileSync(join(source, "src/value"), "10");
  writeFileSync(join(source, "bench.sh"), 'printf \'METRIC {"name":"bench_ms","value":%s,"unit":"ms"}\\n\' "$(cat src/value)"\n');
  const env = { ...process.env, GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: "/dev/null", GIT_AUTHOR_NAME: "Metimer", GIT_AUTHOR_EMAIL: "metinamerwane@gmail.com", GIT_COMMITTER_NAME: "Metimer", GIT_COMMITTER_EMAIL: "metinamerwane@gmail.com" };
  const git = (...args) => execFileSync("git", ["-C", source, ...args], { env, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  git("init", "--template="); git("add", "."); git("commit", "-m", "adapter fixture");
  const config = JSON.parse(readFileSync(template));
  config.session_id = "pi-test";
  config.source = { repository: source, commit: git("rev-parse", "HEAD") };
  config.scope.protected_paths = ["bench.sh"];
  config.execution.network = "allowed";
  config.checks = [{ executable: "/bin/sh", args: ["-c", "exit 0"], cwd: "." }];
  config.benchmark = { executable: "/bin/sh", args: ["bench.sh"], cwd: "." };
  config.sampling.warmup = 0;
  config.budget.deadline_unix_ms = Date.now() + 120000;
  config.budget.command_timeout_seconds = 30;
  if (blockedHook) {
    config.execution.hooks.before = [{ executable: "/bin/sh", args: ["-c", "mkdir -p build; sleep 30 & printf '%s' $! > build/child; wait"], cwd: "." }];
  }
  const path = join(temp, "config é.json");
  writeFileSync(path, JSON.stringify(config));
  const cli = (...args) => JSON.parse(execFileSync(binary, [...args, "--root", root, "--json"], { encoding: "utf8" }));
  cli("init", "--config", path, "--operation-id", "init");
  return { root, source, cli, directory: join(root, ".auto/engine/sessions/pi-test") };
}

async function load(root) {
  const loaded = await discoverAndLoadExtensions([extension], root, join(root, "empty-agent-dir"));
  assert.deepEqual(loaded.errors, []);
  assert.equal(loaded.extensions.length, 1);
  loaded.runtime.flagValues.set("autoresearch-engine", binary);
  loaded.runtime.flagValues.set("autoresearch-root", root);
  loaded.runtime.flagValues.set("autoresearch-session", "pi-test");
  const runner = new ExtensionRunner(loaded.extensions, loaded.runtime, root, SessionManager.inMemory(root), {});
  const errors = [];
  runner.onError((error) => errors.push(error));
  const ext = loaded.extensions[0];
  assert.deepEqual([...ext.tools.keys()], ["autoresearch_engine"]);
  assert.equal(ext.handlers.has("agent_end"), false);
  const tool = ext.tools.get("autoresearch_engine").definition;
  const call = (params, signal, onUpdate) => tool.execute("test-call", params, signal, onUpdate, runner.createContext());
  return { ext, runner, call, errors };
}

async function waitFor(check) {
  for (let i = 0; i < 500; i++) {
    const value = check();
    if (value) return value;
    await delay(20);
  }
  assert.fail("Expected the supervised hook to start.");
}

test("Pi 0.85.1 loads the adapter and executes the Rust candidate workflow", async (t) => {
  const f = fixture(t);
  const { call } = await load(f.root);
  await assert.rejects(call({ action: "baseline" }), /operation_id/);
  await assert.rejects(call({ action: "status", candidate: "extra" }), /Unexpected/);
  await call({ action: "workspace", operation_id: "workspace", local_changes: "exclude" });
  const baseline = await call({ action: "baseline", operation_id: "baseline" });
  assert.equal(baseline.details.evaluation.report.decision, "qualified");
  const prepared = await call({ action: "prepare-candidate", operation_id: "prepare", candidate: "fast", hypothesis: "reduce fixture work" });
  writeFileSync(join(prepared.details.artifact.path, "src/value"), "5");
  await call({ action: "seal", operation_id: "seal", candidate: "fast" });
  const evaluated = await call({ action: "evaluate", operation_id: "evaluate", candidate: "fast" });
  assert.equal(evaluated.details.evaluation.report.decision, "kept");
  assert.match(evaluated.content[0].text, /evaluate: kept/);
  const repeated = await call({ action: "evaluate", operation_id: "evaluate", candidate: "fast" });
  assert.equal(repeated.details.evaluation.already_applied, true);
  const history = await call({ action: "history" });
  assert.equal(history.details.evaluations.length, 2);
  const report = await call({ action: "report" });
  assert.equal(report.details.evaluation.sha256, evaluated.details.evaluation.sha256);
  await call({ action: "stop", operation_id: "stop" });
  const resumed = await call({ action: "resume", operation_id: "resume" });
  assert.equal(resumed.details.session.state.attempts_used, 1);
  assert.equal(readFileSync(join(f.source, "src/value"), "utf8"), "10");
  const exported = await call({ action: "export-candidate", operation_id: "export", candidate: "fast", output: join(f.root, "export é") });
  assert.equal(exported.details.artifact.evaluated, false);
});

test("a stop between negotiation and launch prevents the experiment", async (t) => {
  const f = fixture(t);
  const { call, ext, runner } = await load(f.root);
  await call({ action: "workspace", operation_id: "workspace", local_changes: "exclude" });
  let cancellation;
  await assert.rejects(call({ action: "baseline", operation_id: "not-launched" }, undefined, () => {
    cancellation = ext.commands.get("autoresearch-stop").handler("", runner.createContext());
  }), /cancelled before launch/);
  await cancellation;
  assert.equal((await call({ action: "history" })).details.evaluations.length, 0);
  const state = (await call({ action: "status" })).details.session.state;
  assert.equal(state.active_ms_used, 0);
  assert.equal(state.execution, null);
});

for (const method of ["abort", "command", "session_before_switch", "session_before_fork", "session_before_tree", "session_shutdown"]) {
  test(`${method} cancels a live hook, cleans its child and preserves engine accounting`, async (t) => {
    const f = fixture(t, true);
    const { call, runner, ext, errors } = await load(f.root);
    await call({ action: "workspace", operation_id: "workspace", local_changes: "exclude" });
    const controller = new AbortController();
    const pending = call({ action: "baseline", operation_id: "blocked" }, controller.signal);
    // Attach a rejection handler immediately, even if a regression interrupts the host.
    const settled = pending.then((result) => ({ result }), (error) => ({ error }));
    const marker = await waitFor(() => {
      try {
        const marker = JSON.parse(readFileSync(join(f.directory, "process.json")));
        return marker.group_pid ? marker : undefined;
      } catch { return undefined; }
    });
    const childPath = join(f.directory, "executions", `run-${(await import("node:crypto")).createHash("sha256").update("blocked").digest("hex")}`, "reference/build/child");
    const child = await waitFor(() => existsSync(childPath) && Number(readFileSync(childPath, "utf8")));
    await assert.rejects(call({ action: "status" }), /busy/);
    if (method === "abort") controller.abort();
    else if (method === "command") await ext.commands.get("autoresearch-stop").handler("", runner.createContext());
    else await runner.emit({ type: method, reason: "new" });
    const outcome = await settled;
    assert.ifError(outcome.error);
    assert.equal(outcome.result.details.evaluation.report.decision, "cancelled");
    assert.equal(outcome.result.details.session.state.status, "stopped");
    assert.ok(outcome.result.details.session.state.active_ms_used > 0);
    assert.equal(existsSync(join(f.directory, "process.json")), false);
    for (const pid of [-marker.group_pid, child]) {
      await waitFor(() => {
        try { process.kill(pid, 0); return false; }
        catch (error) { assert.equal(error.code, "ESRCH"); return true; }
      });
    }
    assert.deepEqual(errors, []);
    if (method !== "session_shutdown") {
      // A tree navigation or cancelled switch may never emit session_start.
      assert.equal((await call({ action: "status" })).details.session.state.status, "stopped");
    }
    // Session start/reload does not resume the Rust session or launch another loop.
    await runner.emit({ type: "session_start", reason: "new" });
    const state = await call({ action: "status" });
    assert.equal(state.details.session.state.status, "stopped");
    assert.equal(state.details.session.state.sequence, outcome.result.details.session.state.sequence);
  });
}
