# Rust engine development

The engine has two Cargo workspace members:

- `autoresearch-core`: contracts, persistent sessions, budget accounting and isolated snapshots.
- `autoresearch-cli`: the `autoresearch` binary, with text and versioned JSON output.

The workspace uses Rust 2021 and supports Rust 1.81 or later. `Cargo.lock` is
committed, including dependency versions compatible with this minimum. The first
build may download those dependencies. Neither crate is published to a registry.

## Available commands

Build from a full source checkout:

```sh
cargo build --workspace --locked
./target/debug/autoresearch doctor --json
./target/debug/autoresearch validate --config examples/session.json --json
./target/debug/autoresearch schema
```

`doctor` checks the compiled platform and Git executable discovery on PATH. It
does not execute Git or verify its version. Its success describes those checks,
not readiness to run experiments.

`validate` checks JSON structure and semantic consistency. It does not inspect
the declared source repository, verify file hashes, run commands, create a
session, or compare the deadline with the current time.

Use a configuration based on `examples/session.json`, replacing its placeholder
repository, commit, workload hash, commands, scope, metric and deadline. To store
session metadata in an existing project directory:

```sh
./target/debug/autoresearch init --config examples/session.json \
  --root /path/to/project --operation-id create-001 --json
./target/debug/autoresearch status --session example-session \
  --root /path/to/project --json
./target/debug/autoresearch resume --session example-session \
  --root /path/to/project --operation-id resume-001 --json
./target/debug/autoresearch stop --session example-session \
  --root /path/to/project --operation-id stop-001 --json
```

Use your configuration's `session_id` in place of `example-session` if changed.
`--root` defaults to the current directory; the configuration file path is
relative to the invocation directory. `init` checks the current deadline and
publishes a new session with a frozen copy of the validated configuration.
It does not create an experimental checkout or verify the source repository.

Every mutation requires an operation ID: 1–128 ASCII letters, digits, hyphens or
underscores. Retry with the same ID after a lost response; use a new ID for a new
operation. A successful retry returns the current session state, which may have
advanced since the original call. It does not repeat the transition. Reusing an
ID for another operation is a conflict. Retrying `init` also requires the same
configuration. Existing sessions are never overwritten.

`resume` moves a created or stopped session to `active`, or abandons an unfinished
reservation while retaining its charge. It starts no process. An active session
with no unfinished reservation needs no resume; use `status`. `stop` records
`stopped` and retains any unfinished reservation's charge. It currently requires
the session lock and cannot signal a running worker. Supervisors and cancellation
requests belong to the process execution implementation.

The Python skills remain the operational optimization workflow. The Rust engine
manages configuration, session metadata and isolated Git snapshots. Workspace
commands invoke bounded Git plumbing; they do not run project checks, hooks,
benchmarks or result acceptance.

## Configuration version 2

Version 2 adds execution policies and measurement declarations to the initial
unpublished version 1 contract. Version 1 is rejected; there is no automatic
migration. Create a new configuration from the version 2 template.

The structural Draft 7 schema is checked in at
[`schemas/session-v2.schema.json`](../schemas/session-v2.schema.json). Regenerate it
from the Rust types after building:

```sh
./target/debug/autoresearch schema > schemas/session-v2.schema.json
```

| Field | Rule |
| --- | --- |
| `session_id`, `goal` | Explicit identity and objective; ID is 1–64 ASCII letters, digits, hyphens or underscores. |
| `source` | Repository path and full 40- or 64-character hexadecimal commit ID. |
| `scope` | Literal relative editable, protected and generated paths; trailing slash denotes a directory prefix. Generated paths must be disjoint from editable, protected and reserved paths. |
| `scope.protected_sha256` | Explicit SHA-256 hashes for protected files. An empty map is permitted at this configuration stage. |
| `checks`, `benchmark` | Behavior checks and measurement command as executable, argument list and relative working directory; no implicit shell. |
| `metric` | Name, unit, direction, value domain and finite positive minimum useful improvement in that unit. |
| `sampling` | 3–31 measured runs, 0–31 warmups, 3–31 baseline rounds, balanced ordering, cache policy, SHA-256 workload identity and explicit seeds. |
| `sampling.cache` | `none`, `cold` or `warm`; cache paths must lie within generated paths. `none` requires an empty path list. |
| `execution` | Inherited and explicitly set environment variables, `disabled` or `allowed` network policy, setup commands and before/after hooks. Environment names must be valid and distinct. |
| `secondary_constraints` | Up to 32 distinct metrics with explicit units, domains and `at_most` or `at_least` bounds. |
| `budget` | Positive attempt, active-time, deadline and byte limits. Command timeout cannot exceed active time; output allowance cannot exceed artifact storage. |
| `commit_policy` | `never` or `explicit`. |

These policies are declarations for the future runner. In particular,
`network: "disabled"` does not install a network sandbox. Workspace preparation
now verifies local repository identity, the requested commit, protected hashes
and supported paths. It records the captured snapshot and explicit local-change
policy. Measurement ordering, cache handling, hooks, process quotas and commit
policy still require execution support.

Inputs are limited to 1 MiB. Serde rejects unknown and duplicate fields, including
duplicate keys in hash and environment maps. Semantic validation produces a
read-only `ValidatedConfig`. Tests check the generated schema against the checked-in
file and compare schema/runtime decisions on structural cases. Cross-field rules,
byte-length limits, path policy, duplicate JSON keys and finite-number checks
remain runtime gates; passing JSON Schema alone does not authorize a session.
The deadline is checked separately at initialization and before new work/resume.

## Storage and recovery

A session uses a local Linux or macOS filesystem:

```text
.auto/engine/sessions/
  .init.lock
  <session_id>/
    session.json
    events.jsonl
    state.json
    lock
    events.partial-<sha256>.jsonl   # only after explicit tail recovery
```

Initialization writes and synchronizes a temporary directory, then publishes it
by renaming within the session parent under the initialization lock. A failed
initialization can leave parent directories; abrupt process death before
publication can leave an unpublished `.creating-*` directory. It is never opened
as a session and there is no automatic garbage collection yet.

The stable `lock` file has an exclusive OS advisory lock for the lifetime of
`SessionGuard`. Another cooperating writer fails with `session_busy`. The OS
releases the lock when the owner exits, including a forced termination. Normal
guard destruction explicitly unlocks it before closing the file, so a descriptor
temporarily duplicated by concurrent process spawning cannot prolong ownership.
Lock files must not be deleted or replaced to recover a session. `status` also takes
this lock, so it returns busy while another process owns the guard.

The journal is authoritative. Each strict, versioned event contains a sequence,
operation ID, timestamp, previous-record hash and SHA-256 checksum. New events
are appended and synchronized before acknowledging the transition; `state.json`
is then replaced atomically from a synchronized temporary file. Parent directory
changes are synchronized. A retry reconfirms journal durability and repairs the
projection without appending the event again.

On open, the engine validates and replays the entire journal and compares the
stored configuration fingerprint. Missing or stale projections are recoverable;
`status` reports their condition without rewriting them. A projection ahead of
the journal blocks recovery because committed events appear to be missing.
A failed projection write reports `projection_failed`: the journal transition
has already committed. Resolve the filesystem problem and retry the same
operation ID. An uncertain journal append requires closing and reopening the guard.

A non-newline-terminated final record blocks normal opening. After inspecting it,
explicitly preserve and remove that tail with:

```sh
./target/debug/autoresearch resume --session example-session \
  --root /path/to/project --operation-id recovery-001 --repair-tail --json
```

The engine first validates the complete prefix and checks that recovery would
not discard events recorded in the projection. It preserves the original journal
under its SHA-256 filename before replacing it with the complete prefix. Invalid
complete records, broken hashes, unknown versions and invalid transitions always
block recovery. Tail repair happens before the resume transition, so the backup
and repaired journal may exist even if resume then fails, for example at an
expired deadline. Repair alone creates no new budget or accepted result.

Configuration and projection reads are limited to 1 MiB, journal reads/appends
to 64 MiB, and each journal record to 16 KiB. Recovery backups are additional local
files; the future artifact quota and retention implementation must account for
them. Owned directories reject symlinks; opened session files reject symlinks,
hard links and non-regular files. New owned directories use mode 0700 and new
metadata files 0600. Snapshot files use Git's normal 0644/0755 modes inside private
directories. The frozen configuration includes explicitly set environment
values, so keep credentials out of it.

These guarantees target cooperating processes on a local filesystem. They do
not cover NFS/distributed locking, hostile replacement of parent directories,
malicious rewriting by the same OS user, or hardware that violates synchronization
guarantees. The hash chain detects corruption; it is not authentication or an
external backup. Deleting both a journal suffix and its projection cannot be
reliably detected without an external anchor. Process-death recovery is tested;
power-loss behavior has not been qualified.

## Independent workspaces and candidate exports

Workspace commands require Git on PATH and a local non-bare source repository.
They use configuration version 2 without changing it. `source.repository` is
resolved relative to the pilot root selected with `--root`, not relative to the
configuration file or a later invocation directory. The declared commit must
exist and identify a commit object.

```sh
./target/debug/autoresearch workspace --session example-session \
  --root /path/to/project --local-changes exclude --operation-id workspace-001 --json
./target/debug/autoresearch prepare-candidate --session example-session \
  --root /path/to/project --candidate trial-001 --hypothesis "Reduce repeated work" \
  --operation-id prepare-001 --json
# Edit only the candidate directory returned above.
./target/debug/autoresearch seal --session example-session \
  --root /path/to/project --candidate trial-001 --operation-id seal-001 --json
./target/debug/autoresearch export-candidate --session example-session \
  --root /path/to/project --candidate trial-001 --output /path/to/new-bundle \
  --operation-id export-001 --json
```

Choose the local-change policy explicitly when creating the workspace:

- `exclude`: capture the exact declared commit; staged, unstaged and untracked
  changes do not enter the snapshot. A dirty source is allowed and remains intact.
- `include`: require source HEAD to match the declared commit, then capture the
  current bytes of tracked and non-ignored untracked files, including deletions.
  This captures the working-tree view, not a separate staged/index version.
  Ignored untracked files, generated paths and local `.auto/` state are excluded.
  Two consecutive captures and a final HEAD check detect changes during capture.

Pause source edits while capturing and candidate edits while sealing. The engine
does not lock an editor; repeated scans cannot establish an atomic snapshot of a
source that another process continuously rewrites.

The engine reads source objects and file lists without refreshing its index. It
builds a new bare Git object database from raw captured bytes, with no shared
objects, hard links, alternates, worktree registration or remote. Only the selected
snapshot is copied, not source commit history. `refs/autoresearch/base` points to
its Git tree. Git objects are a local representation; the journal-bound manifest
and verified snapshot files establish the baseline identity.

Git invocation clears inherited Git configuration environment variables, disables
system/global config, replacement objects, lazy fetching, hooks, fsmonitor,
automatic maintenance and transport protocols. Raw blob ingestion bypasses
filters; export disables external diff and text conversion. Each plumbing process
has a 30-second timeout, bounded stdout/stderr and no implicit shell. This small
Git wrapper is not the future supervisor for arbitrary project processes.

The source's HEAD, index, refs, objects and user files are not changed by these
operations. If the pilot root is also the source root, the explicitly owned
`.auto/engine/` metadata is created there; use a separate pilot directory to keep
all engine storage outside the source. No branch is committed or promoted.

Published storage extends the session directory:

```text
artifacts/
  workspace/
    manifest.json
    tree/                    # frozen initial snapshot
    repository.git/          # independent Git object database
  candidate-<id>/
    manifest.json            # identity, hypothesis and baseline fingerprint
    tree/                    # editable candidate, without .git
  sealed-<id>/
    manifest.json
    tree/                    # copied, verified candidate snapshot
  export-<operation-hash>/
    manifest.json            # export receipt and reproduction fingerprints
```

A candidate ID contains 1–64 ASCII letters, digits, hyphens or underscores. Only
one unsealed candidate can be prepared at a time. After sealing, a new candidate
starts from the frozen initial baseline; accepted-reference promotion is not
implemented yet. Baseline here means captured source, not a statistically
qualified performance baseline. Candidate preparation does not reserve an
experiment attempt or execute any configured command.

At sealing, every added, deleted or modified file must lie in the allowed scope
and outside protected paths. Protected files, including executable mode, must
match the captured baseline; supplied protected SHA-256 values are checked at
capture and sealing. Declared generated files are checked for unsafe filesystem
entries, then omitted from the sealed code snapshot and patch. Generated paths
cannot contain files tracked by the declared source commit.

Snapshots support regular UTF-8 file paths, raw binary content and Git's executable
bit. Symlinks (including internal links), hard-linked working files, submodules,
special files, reserved `.git`/`.auto` components, traversal, case-colliding names,
and unsupported path syntax are rejected. Decomposed combining-mark filenames
are rejected conservatively; arbitrary cross-platform Unicode normalization is
not supported. Empty directories, ACLs, ownership and non-Git permission bits are
not preserved. These limits are checked before publishing a candidate snapshot.

Sealing stores an independent copy and its SHA-256 inventory. Later edits to the
editable candidate do not alter that copy. A changed frozen snapshot blocks reuse
or export. There is no `kept` verdict, performance evidence or accepted-result
promotion in this tranche.

An export must target a new directory outside the source and Git/engine metadata.
It contains `base/`, `candidate.patch`, `manifest.json` and reproduction instructions.
The manifest anchors both the base inventory and patch hash. Every generated patch
is applied to a temporary Git index and its resulting tree compared with the
sealed tree before publication. Binary changes and executable modes are preserved;
renames are represented as removal/addition pairs. An unchanged candidate has an
empty patch. The supplied base makes reproduction independent of later source
changes, even when local changes were included initially.

The exported patch verifies code reproduction only. It does not verify behavior,
performance or whether the candidate should be retained. Export is available
while stopped or after the deadline; workspace creation, preparation and sealing
require a non-stopped session with no reservation in flight and an unexpired
deadline. The future runner will account for actual experiment stages.

Artifact manifests are version 1 and limited to 4 MiB; snapshots are limited to
4,096 files and the smaller of 64 MiB or the configured artifact budget. Filesystem
walks have depth/entry limits. Publication checks total bytes under the session's
artifact directory, including candidate copies and staged snapshots, against
`max_artifact_bytes`. Each external bundle has the same byte ceiling separately.
Temporary staging may consume bytes before that final check; this is not a live
disk quota, and external bundles are not part of the session directory's cumulative
limit. General retention, cleanup and process-output quotas remain future work.

Artifacts are synchronized and published before their journal event. An operation
retry can reconcile a fully published artifact after an uncertain journal append;
it never silently regenerates a missing journaled artifact. Retrying preparation
preserves the editable candidate. Export retries verify the existing bundle and
will not overwrite another destination, even an empty one. Directory publication
uses an exclusive destination reservation; interruption may leave `.building-*`,
`.exporting-*`, a complete unjournaled artifact, or an empty destination reservation.
Complete artifacts can be reconciled with the same request/operation ID. Partial
staging or an empty reservation requires inspection; no automatic deletion occurs.

Old metadata-only sessions remain readable: absent artifact projections default
to an empty map. New artifact events require this engine version; older binaries
will reject those events. Configuration version 2 itself is unchanged.

## Budget accounting

The core exposes `reserve_work` and `settle_work` for a future trusted runner;
neither is a public CLI command. One reservation currently consumes one attempt
and charges the smallest of the command timeout, remaining active time and time
to the original deadline. The runner must reserve durably before starting work.
An already-applied reservation must never trigger the same work a second time.

Settlement can return unused time only for the current reservation and only
from trusted monotonic elapsed time within the reservation. An interrupted or
abandoned reservation keeps its entire charge. Stop/resume does not restore
attempts, consumed time or the original deadline. Completed idle time between CLI
calls is not active time. An exhausted budget blocks new work and resume, while
stop remains possible. A regressed wall clock blocks new transitions except stop,
which clamps its timestamp to the last event time.

Multi-stage experiments, baseline costs and process timeout enforcement will
build on this ledger. No actual experiment budget enforcement is claimed before
that runner exists.

## CLI output and exit codes

`--json` produces one JSON object on stdout using envelope `schema_version: 1`;
this is independent of configuration version 2. `schema` always emits the schema
object itself. Session responses contain state, remaining budgets, deadline,
clock/recovery/projection flags and `commands_executed: false`. Resume/stop also
report `already_applied`. Configuration values are not echoed in parse errors or
status output. Workspace responses instead include `experiments_executed: false`
and an `artifact` object with path, SHA-256, file count, retry status and
`evaluated: false`. Session state lists journaled artifact fingerprints. Text
errors go to stderr.

| Code | Meaning |
| --- | --- |
| 0 | Requested operation succeeded, including an idempotent retry. |
| 1 | Filesystem I/O failed. |
| 2 | Invalid arguments, identifier or configuration. |
| 3 | Unsupported session platform, or `doctor` could not find Git on a supported platform. |
| 4 | Conflicting operation or session lock busy. |
| 5 | Corrupt session/artifact, unsafe path, storage limit or failed state projection. |
| 6 | Attempt/time budget or original deadline exhausted. |
| 7 | Clock moved backwards or outside the supported range. |
| 8 | Git plumbing failed, source identity is invalid, or a snapshot violates file policy. |

## Checks and remaining work

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 -m unittest discover -s tests -v
node --test tests/legacy/jsonl.test.mjs
```

The Rust suite covers structural and semantic validation, CLI invocations,
operation retries, budget exhaustion, missing projections, corrupt journals,
explicit tail repair, links and process death while holding a reservation. Workspace
tests compare all source files and Git metadata before/after operations, exercise
dirty-file policies, protection checks, publication failures and exact patch
reproduction, including binary content and executable modes. One
ignored test is a subprocess fixture invoked by its parent test; it is exercised
as part of the normal suite.

Node.js 24 is needed only for characterization of the archived reader. The Rust
binary needs neither Node.js nor Pi. CI defines Linux/macOS jobs for Rust 1.81 and
stable, Python 3.10/3.13, plus a Node 24 job; remote execution remains unverified. This tranche was checked locally on macOS
with Rust 1.81 and Git 2.47.0.

The archive remains unchanged and is not imported automatically. Historical
unknown outcomes and malformed records never become verified Rust results.
See `THIRD_PARTY_NOTICES.md` for attribution.

The next implementation is process supervision: declared environment, owned
process groups, cancellation, timeouts, output and storage quotas, and accounting
for multi-stage experiments. Baseline qualification, comparisons, acceptance and
the Pi adapter follow that foundation. See `IMPLEMENTATION_PLAN.md` for the complete sequence.
