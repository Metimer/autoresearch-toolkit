# Portable optimization

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
6. Append one JSON object per line to .auto/portable-log.jsonl using the version 1
   record below. Preserve real samples, exit status, UTC time and measured duration;
   never invent successful measurements for a failed command.
   Record retained uncommitted state in .auto/portable-state.md for resumption.
   Exclude credentials and sensitive raw workload data from logs.

## Portable journal version 1

Use these exact field names for a new journal (the values below are illustrative):

```json
{"schema_version":1,"iteration":1,"timestamp_unix_ms":1767225600000,"hypothesis":"Remove repeated allocation","head":"0000000000000000000000000000000000000000","changed_paths":["src/main.rs"],"baseline_samples":[10,10,10],"candidate_samples":[8,8,8],"metric":"bench_ms","unit":"ms","direction":"lower","check_exit_status":0,"decision":"keep","reason":"Measured improvement with passing checks","elapsed_seconds":1.5}
```

Replace every value with observed facts. Use a positive increasing iteration,
a current UTC timestamp in milliseconds, full Git HEAD, finite raw samples,
`lower`/`higher` direction, an integer check exit status and nonnegative elapsed
seconds. All listed fields are required. A failed measurement can leave samples
empty and use `discard` or `inconclusive` with an accurate reason; it cannot become
`keep`. Record unavailable checks explicitly in the reason with a nonzero failure
status; do not imply they ran successfully.

Older portable journals had no enforced schema. Inspect their existing structure
before appending, and preserve their original format during a resume. Do not mix
versioned and unversioned records or rewrite historical lines. If migration to
Rust is requested, use the engine's explicit historical import into a new session;
imported portable verdicts remain unverified until new engine measurements.

## Stop and resume

Stop on user stop, exhausted limits, unavailable prerequisites, unstable baseline,
unexplained external edits, or required new permission. Do not silently restart
the budget or schedule a later run after usage limits. Resume only when invoked
again and reconcile HEAD/diff/journal first.

Finish with attempted/kept/discarded/inconclusive counts, measured best result,
noise caveat, checks, exact remaining diff and whether anything was committed.
Leave the user's other files and .auto evidence intact.
