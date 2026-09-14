---
name: autoresearch-run
description: Run or resume a bounded autoresearch optimization session with an existing .auto baseline, measuring one hypothesis at a time and retaining only validated improvements. Not a general refactoring or audit workflow.
---

# Autoresearch Run

This is an agent-driven workflow, not a background engine. Use the host's normal
editing and shell tools. Do not assume Pi's init_experiment, run_experiment,
log_experiment, automatic resume hooks or dashboard exist.

## Entry gate

Read repository instructions, .auto/prompt.md, .auto/context.md, baseline evidence
and previous portable journal .auto/portable-log.jsonl when present. Do not parse
a Pi log as this journal or overwrite an existing session.

Starting requires user authorization to optimize, an explicit allowed file scope,
a maximum iteration count and a wall-clock deadline. Ask for missing limits.
A request to explain/prepare this workflow is not permission to run it.
No unattended scheduler, network access, dependencies, commit or push is implied.

Record HEAD and file state. Require an isolated experimental checkout with no
unrelated changes. If already resuming owned edits, inspect and attribute them
before acting. Check the original workload and correctness baseline still apply.
Requalify a stale/missing baseline before optimization; never call it a speedup
when the machine, inputs, checks, dependencies or methodology changed.

## Bounded loop

For each remaining iteration, while time remains:
1. State one hypothesis, affected files and expected metric effect.
2. Record the pre-experiment state and exact patch. Make the smallest scoped edit.
   Do not change tests, workload, checks, metric calculation or quality settings
   to improve the score. No feature removal or behavior regressions.
3. Execute checks and measure with timeouts bounded by the remaining session
   budget. A non-zero exit, timeout, missing/non-finite metric, changed invariant
   or out-of-scope diff disqualifies the candidate.
4. Compare like-for-like samples to the best accepted baseline. Re-measure close
   results; do not claim gains below observed noise. Use the declared direction,
   not always lower-is-better. A simplicity-only win needs an explicitly agreed
   secondary acceptance criterion.
5. Keep a validated improvement in the experimental checkout; commit only if
   requested, with the user's author/co-author policy. Otherwise revert ONLY this
   iteration's own patch using precise edits. Do not use broad git checkout,
   reset --hard, clean, stash or filesystem deletion. If ownership is uncertain,
   stop and ask instead of guessing.
6. Append one JSON line to .auto/portable-log.jsonl: iteration, UTC time, hypothesis,
   HEAD, changed paths, baseline/candidate raw samples, metric/unit/direction,
   check exit status, decision (keep/discard/inconclusive), reason and elapsed time.
   Record retained uncommitted state in .auto/portable-state.md for resumption.
   Exclude credentials and sensitive raw workload data from logs.

## Stop and resume

Stop on user stop, exhausted limits, unavailable prerequisites, unstable baseline,
unexplained external edits, or required new permission. Do not silently restart
the budget or schedule a later run after usage limits. Resume only when invoked
again and reconcile HEAD/diff/journal first.

Finish with attempted/kept/discarded/inconclusive counts, measured best result,
noise caveat, checks, exact remaining diff and whether anything was committed.
Leave the user's other files and .auto evidence intact.
