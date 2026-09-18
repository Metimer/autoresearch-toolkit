# Rust optimization

Use the exact compatible binary as `$engine`, the recorded pilot as `$pilot`, and
its session ID as `$session`. Quote each path; commands take argv directly.

```sh
"$engine" status --session "$session" --root "$pilot" --json
"$engine" history --session "$session" --root "$pilot" --json
"$engine" report --session "$session" --root "$pilot" --json
```

Read the frozen contract at `$pilot/.auto/engine/sessions/$session/session.json`
for scope and methodology; never modify it. Reconcile the CLI's accepted snapshot,
qualification, remaining limits and pending execution. `next_action` is guidance,
not authorization. `report` defaults to the qualified accepted reference; select
any completed evaluation using the `evaluation_key` returned by `history`.
If a stored report is corrupt, stop rather than reconstructing a favorable result.

`resume` activates metadata without starting work or replenishing budgets. On
recovery, retry the original request first when a completed report can be
reconciled. Otherwise inspect the interrupted work before using a new resume
operation to abandon its reservation; its time remains charged. Do not delete
locks, process markers or snapshots. The runner refuses recovery while the
recorded process group may still exist.

A missing/stale qualification requires `baseline` with a new operation ID before
a new trial, using the existing contract and remaining time. Do not change workload,
checks, quality settings, dependency versions or metric calculation to improve the
score. Method/environment changes may require fresh qualification or a separately
authorized new session. Never reset the existing budget to continue a loop.

## One hypothesis, one sealed evaluation

```sh
"$engine" prepare-candidate --session "$session" --root "$pilot" \
  --candidate trial-001 --hypothesis "Reduce repeated work" \
  --operation-id prepare-001 --json
```

Edit only the returned `artifact.path` within the allowed scope. It is a private
copy of the accepted reference; do not edit the original source or sealed tree.
Only one unsealed candidate can exist. On resume, retry its original prepare
request to recover the same path without discarding edits. Preserve the recorded
hypothesis; if its scope cannot be repaired safely, stop and retain evidence.

```sh
"$engine" seal --session "$session" --root "$pilot" \
  --candidate trial-001 --operation-id seal-001 --json
"$engine" evaluate --session "$session" --root "$pilot" \
  --candidate trial-001 --operation-id evaluate-001 --json
```

The engine owns checks, sampling, constraints, confirmation and promotion. Read
`evaluation.report.decision`, never infer success from exit zero or submit a
self-assigned `kept` verdict. `kept` promotes this exact sealed snapshot;
`discarded`, `inconclusive`, `failed` and `cancelled` do not. Retain their evidence.
Do not retry marginal results with new IDs until one happens to pass. A repeated
request with the original ID returns its evidence without executing again.

After a rejection, prepare the next hypothesis from the engine's accepted
reference; no manual rollback of the original source is needed. Every candidate
evaluation consumes one attempt; setup, hooks, checks, warmups and confirmation
all consume process time. Stop at the smaller of user-authorized and stored limits.
Treat `baseline_stale` as a blocker for new comparisons until requalified.

## Stop and export

```sh
"$engine" stop --session "$session" --root "$pilot" \
  --operation-id stop-001 --json
```

A busy supervisor returns `cancellation_requested`; wait for its evaluation result
or inspect status once the lock is released. The request acknowledges intent,
not completed shutdown. A fresh stop request needs a fresh operation ID; retrying
an old request remains bound to its original execution. SIGINT/SIGTERM also request
cleanup. Process groups are not a sandbox against untrusted commands.

Use `history`, `report` and `status` for the final account. Export the accepted
candidate ID only after checking `session.state.accepted` (strip `sealed-`):

```sh
"$engine" export-candidate --session "$session" --root "$pilot" \
  --candidate trial-001 --output "$pilot/accepted-patch-001" \
  --operation-id export-001 --json
```

Use a new destination outside the source and engine metadata. If no candidate was
accepted, report that rather than exporting a rejection as the best result.
The bundle verifies code reproduction, not performance; report the evaluation
key/hash separately. No source commit or branch merge is implicit. Preserve the
session and provide its mode, executable, pilot/session IDs and remaining budget
for an explicit future resume.
