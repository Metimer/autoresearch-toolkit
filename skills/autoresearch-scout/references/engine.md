# Rust preparation

Use a compatible `autoresearch` executable on Linux/macOS (or inside WSL). The
binary is installed separately; exported skill bundles do not contain it. Quote
paths containing spaces. In the examples, `$engine` is its absolute path, `$pilot`
is an existing directory for local session storage, and `$session` is the chosen
ASCII session ID. The pilot can be outside the source repository.

## Freeze the contract

Adapt the [version 2 template](../assets/session.json) to observed commands and
store a draft outside captured source, for example `$pilot/.auto/session-draft.json`.
`"disabled"` network is the safe template default but blocks execution in this
runner: select `"allowed"` only when trusted local commands and their network
behavior fit the existing authorization. If network isolation is required, report
that this backend cannot supply it. Do not silently weaken the restriction.

Set a real repository path (relative to the pilot, or absolute), full Git commit,
input SHA-256 declaration, scope, protected tests/benchmarks and an actual future
UTC deadline in milliseconds. The contract needs both attempt and process-time
budgets; reuse limits already supplied, or clarify missing limits before init.
`active_seconds` covers commands, not the agent's reasoning/editing time; the
original wall-clock deadline remains the outer limit. Baseline consumes time but
no attempt. Initializing the contract does not itself authorize optimization.

Declare exact argv/cwd for checks, benchmark, setup and hooks. No placeholder
checks, inferred installations or shell interpolation. Environment is cleared
except declared variables; HOME/TMPDIR are private. Toolchains and dependencies
must actually work in that environment. Declare writable generated build/cache
paths separately from allowed source edits. No tracked files in generated paths.
Protect benchmark logic, fixtures, checks, manifests and lockfiles. For a new
benchmark harness, include it deliberately in the captured source before freezing.
`exclude` omits uncommitted harness files; `include` captures tracked modifications
and non-ignored untracked files and must reflect the intended source input.

Each benchmark invocation emits one raw observation for each declared metric:

```text
METRIC {"name":"bench_ms","value":12.4,"unit":"ms"}
```

All secondary metrics are required with their own units and bounds. Do not feed
the portable helper's `METRIC name=value` lines to the engine, or wrap an already
aggregated portable series as a single raw observation. Use a project benchmark
that implements the JSON protocol, with unchanged workload semantics.

Use at least five measured runs, at least three baseline rounds, explicit seeds,
cache policy and a finite positive minimum useful improvement in the metric unit.
Warm mode requires warmups. Avoid shared writable caches. The workload hash is a
caller declaration; verify external inputs and record provenance yourself.

## Initialize and qualify

```sh
"$engine" validate --config "$pilot/.auto/session-draft.json" --json
"$engine" init --config "$pilot/.auto/session-draft.json" --root "$pilot" \
  --operation-id create-001 --json
"$engine" workspace --session "$session" --root "$pilot" \
  --local-changes exclude --operation-id workspace-001 --json
"$engine" baseline --session "$session" --root "$pilot" \
  --operation-id baseline-001 --json
"$engine" status --session "$session" --root "$pilot" --json
"$engine" history --session "$session" --root "$pilot" --json
"$engine" report --session "$session" --root "$pilot" --json
```

Choose `include` in the workspace call only when that capture policy is intended.
A mutation needs a unique operation ID; reuse it only to retry that same request.
`validate` does not execute commands or qualify the workload. `init` freezes the
contract; never edit the stored config, journal, projections or reports.

Only `evaluation.report.decision == "qualified"` establishes the baseline.
Exit zero also covers completed failed/inconclusive/cancelled operations. An
unstable baseline blocks optimization: inspect its reason and samples, do not
apply the portable percentage-noise rule. Use `history` and `report --evaluation`
for failed reports; default `report` selects the qualified reference if one exists.
Methodology changes require a separately authorized new session and fresh proof,
not editing the frozen contract or replenishing its budget.

For an existing Rust session, begin with `status`, `history` and `report` instead
of init. `next_action` is a hint, not permission. A stopped session needs `resume`
within existing limits. If recovery is required, inspect pending evidence first:
retry the original baseline/evaluate request to reconcile a complete report;
resume abandons unfinished work and retains its reservation. Never delete process
markers or locks to force recovery. A live/uncertain process group blocks resume.

Record executable path/version, pilot/session, qualified report key/hash, source
capture policy, metric/noise, scope and remaining limits for the run handoff.
