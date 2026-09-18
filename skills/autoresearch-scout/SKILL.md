---
name: autoresearch-scout
description: Prepare a repository for measured optimization. Discover checks and benchmarks, define a protected workload and qualify a baseline using the Rust engine or portable tools. Preparation alone does not authorize optimization.
---

# Autoresearch Scout

Inspect the target repository and prepare a reproducible baseline. Read repository
instructions, record HEAD and dirty state, and discover actual checks, benchmarks,
hot paths and protected files from manifests, CI and source. An audit-only request
requires no session writes or command execution.

For preparation, establish the primary metric, direction, unit, useful gain,
workload, scope and time/attempt limits from the request and evidence. Ask only
for missing information that affects the contract. Preserve unrelated changes;
no dependency installation, branch switch, stash, commit or remote action follows
merely from selecting this skill.

## Select the session mode

Inspect existing `.auto/` before writing; reject symlinked session paths. A session
under `.auto/engine/sessions/` is Rust-owned. A portable session uses
`.auto/prompt.md`, `.auto/context.md` and optional `portable-log.jsonl`. If several
sessions or both formats exist, identify the intended session before modifying it.
Never overwrite, reset budgets or reinterpret one format as another.

For Rust, use the user's binary path or discover `autoresearch` on PATH. Invoke
that exact executable with `doctor --json`; require JSON `schema_version: 1` and
`capabilities.run_experiments: true` and `capabilities.inspect_evaluations: true`.
Discovery alone is not compatibility proof.
Do not download, install or build an engine implicitly.

- Existing Rust session, or Rust explicitly requested: read
  [Rust preparation](references/engine.md). If the binary is absent, incompatible
  or unusable, report the blocker and preserve the session. No portable fallback.
- Existing portable session: read [portable preparation](references/portable.md).
- New session with a compatible engine and no explicit portable preference: use
  Rust and announce the mode. Otherwise announce that the portable workflow uses
  agent-enforced scope/session budgets, then read the portable reference. If Rust
  was requested, its absence remains a blocker.

Keep `.auto/` local and respect the repository's ignore policy. Never force-add
ignored evidence. If it is already tracked, report that ignoring does not untrack it.

## Handoff

Report the chosen mode, session/pilot path, source identity, metric and noise,
protected/allowed scope, remaining budget and any blocker. Preparation alone ends
here. If optimization is already authorized, continue within that authorization;
otherwise wait for a run request. Do not start Pi hooks or an unattended loop.
