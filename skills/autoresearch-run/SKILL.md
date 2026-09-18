---
name: autoresearch-run
description: Run or resume a bounded optimization session from a prepared baseline, using Rust-owned evaluations or the portable workflow. Test one hypothesis at a time and retain only validated improvements; not a general refactoring or audit workflow.
---

# Autoresearch Run

Use the existing session and the user's authorization to optimize. Establish its
allowed file scope, maximum attempts and deadline before editing. Preparation or
an explanation request alone does not authorize experiments. Prior authorization
remains valid; ask only for missing limits. No implicit dependency installation,
external service, unattended scheduling, commit or push.

## Select the existing mode

Read repository instructions and inspect `.auto/` without overwriting it. Reject
symlinked session paths. Rust sessions live under `.auto/engine/sessions/`;
portable sessions use `.auto/prompt.md`, `.auto/context.md` and
`.auto/portable-log.jsonl`. Resolve which session is intended if there are several.
A Pi log is neither of these formats and cannot establish current proof.

- For a Rust session, read [Rust optimization](references/engine.md). Use the
  recorded/user-supplied executable, or discover `autoresearch` on PATH and check
  `doctor --json`: JSON version 1, `capabilities.run_experiments: true` and
  `capabilities.inspect_evaluations: true`. If unavailable or incompatible, stop
  and report the blocker; keep evidence intact. Never continue it as portable.
- For a portable session, read [portable optimization](references/portable.md).
  Announce that the agent enforces overall scope and budgets. A newly installed
  Rust binary does not migrate this session automatically.
- Without a prepared session, establish a baseline within the authorized scope
  before optimization. Do not invent qualification or import historical verdicts.

Record the selected mode, pilot root and session ID in the handoff. Use one mode
per session. Tools exposed by the host do not imply that Pi experiment tools,
automatic resume hooks or a dashboard exist.

## Finish

Stop when requested, at exhausted limits, on unstable reference measurements or
unexplained edits, or when a necessary action exceeds existing authorization.
Report attempts and decisions, best accepted result, evidence and patch paths,
remaining budget and commit status. Resume requires a new invocation and preserves
the original limits. Leave unrelated files and session evidence intact.
