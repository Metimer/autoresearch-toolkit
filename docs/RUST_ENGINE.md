# Rust engine development

The engine has two Cargo workspace members:

- `autoresearch-core`: contracts, sessions, isolated snapshots, supervised execution and measured decisions.
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
`stopped` and retains any unfinished reservation's charge. While a supervisor owns
the writer lock, `stop` writes an atomic, execution-bound cancellation request.
A repeated cancellation request stays bound to its original execution.
Its acknowledgement means cancellation was requested; inspect the completed
evaluation or subsequent status to confirm shutdown.

The skills select the existing session mode; new sessions prefer a compatible
Rust engine unless portable mode is requested. The portable workflow remains
available without a binary. An existing Rust session never silently falls back
to portable execution when the binary is missing or incompatible. Rust `baseline` and `evaluate`
run the configured checks, hooks and benchmarks independently of any agent.
Workspace commands themselves only invoke bounded Git plumbing.

## Inspect results and continue

```sh
autoresearch history --session example-session --root /path/to/project --json
autoresearch report --session example-session --root /path/to/project --json
autoresearch report --session example-session --root /path/to/project \
  --evaluation run-<operation-hash> --json
```

`history` returns completed evaluations sorted by completion timestamp, then key.
Each entry contains `evaluation_key`, `operation_id`, `candidate`, `parent`,
`decision`, `reason`, `finished_unix_ms` and the report's `sha256`. Pending executions
appear in `status`, not as completed history entries. This is a session-local list;
cross-session search and indexing remain planned.

`report` returns the complete `evaluation` object used by baseline/evaluate, plus
its `evaluation_key`. Without `--evaluation`, it selects the current qualified
reference, not the latest rejected experiment. If none is qualified, use history
to choose a completed report explicitly. An unknown selection returns
`report_not_found` (exit 4). Every report's bytes are checked against the fingerprint
in the journal; corruption fails the command instead of silently skipping a result.
The raw stdout/stderr hashes are recorded in the report; inspection does not
re-hash those separate log files or claim their current contents are unchanged.

These commands accept no operation ID and neither mutate evidence nor execute
project commands. They acquire the same exclusive session lock for a consistent
view and return `session_busy` while an evaluation owns it. They work while stopped
or after budget expiry. JSON uses `schema_version: 1` and `commands_executed: false`.

`status` and idle lifecycle responses add the advisory `next_action` field:

| Value | Meaning |
| --- | --- |
| `inspect_recovery` | Inspect interrupted execution/evidence before retrying or abandoning work. |
| `check_clock` | Resolve a regressed wall clock before further mutations. |
| `report_or_export` | An attempt/time limit or deadline is exhausted. |
| `resume_if_authorized` | The session is stopped and still has budget. |
| `workspace` | Capture source with an explicit local-changes policy. |
| `baseline` | Qualify the current reference before trials. |
| `prepare_or_evaluate_if_authorized` | Continue a candidate within the existing authorization and remaining limits. |

Hints do not guarantee the next command will pass its filesystem/process gates,
and never grant authorization. `doctor` advertises `inspect_evaluations` for hosts
using these views. Skills require JSON version 1 plus `run_experiments` and
`inspect_evaluations`; they neither install a missing binary nor migrate formats.
The shipped skill template matches `examples/session.json`. Its placeholders and
network policy require deliberate configuration before execution.

## Historical import

`inspect-legacy --source <jsonl> [--format auto|pi|portable] --json` validates a
bounded, regular UTF-8 file and returns an `import_report` without creating a
session. Auto detection uses record structure, not the filename. Known records
with no version marker are labelled unversioned; a producer version is not guessed.
Parsing errors return exit 2 and a report with line-numbered anomalies; I/O and
byte-limit failures use the usual error envelope/codes. Nothing is
imported if any record fails. Complete final JSON without a newline is preserved
with a warning; a truncated final record or corrupt middle line blocks import.
Blank lines are preserved in the source copy and reported as warnings.

```sh
autoresearch import-legacy --source /path/to/old-log.jsonl \
  --config /path/to/new-session.json --root /path/to/pilot \
  --operation-id import-001 --format auto --json
autoresearch history --session imported-session --root /path/to/pilot --json
autoresearch report --session imported-session --root /path/to/pilot --legacy --json
```

Import requires an explicit version 2 contract with a new session ID and newly
authorized limits. It does not infer executable commands, scope, Git source or
budgets from the legacy journal. The new session is `created`, with zero current
attempts/process time, no workspace, no accepted result and no qualification.
This is a new contract, not continuation of an old budget. An existing session
cannot be overwritten or used as an import destination. Repeating the same import
operation with identical source bytes and contract reconciles the same session;
a different source, contract or initialization ID is a conflict.

The original file is opened read-only. Before publishing the new session, the
engine writes `legacy/source.jsonl` as an exact copy and `legacy/report.json` with
validated declared records, method segments and anomalies. Its first `imported`
journal event binds the configuration and report hash; the report binds the source
hash. Publication uses the existing synchronized staging-directory/rename path.
A crash before publication may leave staging files but no partially initialized
session. Existing-session opens verify both imported files against their hashes;
missing or corrupted evidence blocks use. The report is an integrity receipt,
not authentication against another process with the same user permissions.

`history.evaluations` contains only actual Rust evaluations. Its separate
`historical_import` summary and `report --legacy` expose imported data marked
`historical_unverified`. Default `report` still requires a currently qualified
reference; `--legacy` and `--evaluation` are mutually exclusive. Import does not
execute a script, import an old snapshot, restore uncommitted edits, claim a commit
exists, or translate a declared `keep` into `kept`. The ordinary workspace and
baseline gates remain mandatory. Keep historical instructions as data.

Supported profiles:

| Profile | Records and validation |
| --- | --- |
| `pi_unversioned` | The structural format observed in the archived Pi source: explicit config headers followed by numbered runs. Config requires metric name/unit/direction. Runs require run, commit, metric, status, description and millisecond timestamp. Optional secondary metrics, segment, confidence and object-valued ASI are preserved. Status is exactly `keep`, `discard`, `crash` or `checks_failed`. Config changes retain separate method segments. |
| `portable_v1` | One object per iteration with `schema_version: 1`, `iteration`, `timestamp_unix_ms`, `hypothesis`, `head`, `changed_paths`, `baseline_samples`, `candidate_samples`, `metric`, `unit`, `direction`, `check_exit_status`, `decision`, `reason`, `elapsed_seconds`. Decision is `keep`, `discard` or `inconclusive`. A keep cannot declare failed checks or empty samples. |
| `portable_unversioned` | Same recognized field names without `schema_version`; contextual fields may be absent and are never defaulted. Optional `timestamp` accepts a calendar-valid UTC `YYYY-MM-DDTHH:MM:SS[.fraction]Z` string instead of milliseconds. Incomplete context is explicitly warned about. |

All keys must be distinct, including nested objects. Numbers must be finite and
field types correct; run/iteration IDs must be positive and strictly increasing.
Unknown fields, statuses, explicit versions and mixed formats/version markers are
rejected. Portable files historically had no enforced schema: other spellings or
shapes require a deliberately prepared conversion copy, not guessed aliases.
Native Rust journals use native recovery, not this importer. Historical samples
are not reinterpreted using the new contract's metric direction or units.

Input is limited to 4 MiB, 4096 lines and 64 KiB per record; the stored report has
an 8 MiB ceiling. Source/report/configuration and metadata headroom must fit the
new session storage allowance. Symlinked, hard-linked and non-regular source files
are rejected. No imported code or diagnostic payload is evaluated. Import notices
and the raw source stay local; release/export of historical data is not automatic.

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

Workspace preparation verifies repository identity, the commit, protected hashes
and supported paths. Execution requires `network: "allowed"`; the default
`"disabled"` fails closed because there is no network isolation backend. Commands
must be trusted. Execution additionally requires at least five measured runs,
and warm cache mode requires at least one warmup. No command commits to the
source repository, including when `commit_policy` is `explicit`.

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
files; execution storage checks include them. Automatic retention is still
unimplemented. Owned directories reject symlinks; opened session files reject symlinks,
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
Git wrapper remains separate from the supervisor for project processes.

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
starts from the latest accepted snapshot, or the initial snapshot before any
promotion. Sealing rejects a candidate prepared against an obsolete reference.
Capturing source does not qualify its measurements. Candidate preparation does not reserve an
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
or export. Only evaluation can produce `kept` and promote that sealed snapshot.
Code exports remain cumulative patches against the initial workspace; they do
not embed evaluation reports or certify a verdict.

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
deadline. Evaluation accounts separately for actual process stages.

Artifact manifests are version 1 and limited to 4 MiB; snapshots are limited to
4,096 files and the smaller of 64 MiB or the configured artifact budget. Filesystem
walks have depth/entry limits. Publication checks total bytes under the session's
artifact directory, including candidate copies and staged snapshots, against
`max_artifact_bytes`. Each external bundle has the same byte ceiling separately.
Temporary staging may consume bytes before that final check; this is not a live
disk quota, and external bundles are not part of the session directory's cumulative
limit. Execution adds checks across the whole session and bounded process output.
General retention and cleanup remain future work.

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

## Execution, measurement and acceptance

After creating a workspace, qualify its reference:

```sh
autoresearch baseline --session example-session --root /path/to/project \
  --operation-id baseline-001 --json
```

Prepare, edit and seal a candidate using the workspace commands, then evaluate:

```sh
autoresearch evaluate --session example-session --root /path/to/project \
  --candidate trial-001 --operation-id evaluate-001 --json
```

The engine copies sealed inputs into separate reference/candidate runtime trees.
Each side runs setup, before hooks and mandatory checks. Source inventories are
checked before and after commands; only declared generated paths may change.
After hooks run at the end, with failures recorded separately from the verdict.
An uncertain cleanup or unsettled reservation prevents completion and promotion.

Commands receive only declared inherited/set variables, private `HOME` and
`TMPDIR` directories, `AUTORESEARCH_SAMPLE_INDEX`, `AUTORESEARCH_INPUT_SHA256`,
and `AUTORESEARCH_SEED` when seeds are declared. These names are reserved.
Relative command working directories must be real directories inside the runtime.
There is no implicit shell or inferred dependency installation: declare setup
commands and writable build/cache paths explicitly. For example, Rust builds need
an available toolchain and a declared generated target directory with the relevant
environment settings; a private HOME does not inherit a rustup installation.

The method fingerprint binds configuration, declared environment, OS/architecture
and hashes of directly declared external executables. Relative executables belong
to the snapshot or declared setup output. Transitive tools, libraries, external
inputs and hardware state are not fully captured. `input_sha256` is a caller's
workload identity declaration, not automatic hashing of an external dataset.
Keep the workload fixed and checks/benchmark code protected.

The benchmark emits UTF-8 stdout with exactly one JSON record for every declared
primary and secondary metric. Other log lines are allowed, but any line starting
with `METRIC` must follow this format:

```text
METRIC {"name":"bench_ms","value":12.4,"unit":"ms"}
```

Unknown fields/metrics, duplicate fields/metrics, missing values, wrong units,
non-finite numbers and domain violations fail evaluation. Secondary bounds apply
to every sample, including warmups. The portable helper's `METRIC name=value`
format is not accepted by this runner.

Qualification uses all declared baseline rounds and measured runs, excluding
warmups from the summary. Noise is the maximum minus minimum at constant code;
qualification requires that range not exceed `minimum_improvement`. Comparisons
use at least five pairs with alternating reference/candidate order. The median
of directional paired gains must exceed the minimum useful improvement plus the
larger qualified/current reference noise. Reference drift beyond the declared
minimum or qualified noise invalidates qualification. These are conservative
fixed rules, not a statistical confidence guarantee.

A promising candidate runs exactly one distinct confirmation series, with the
same fixed sample count and its own warmups. Confirmation must pass the gain,
reference stability and candidate noise gates, followed by final behavior checks.
A noisy or insufficient gain is inconclusive; a clear regression or violated
candidate secondary constraint is discarded. No optional stopping or automatic
resampling makes a marginal candidate pass. Cold mode removes only declared
private cache paths before each sample; warm mode preserves them after warmup;
none performs no forced reset. Samples on both sides use the same seed schedule.
A local lock serializes evaluations within a pilot root, not across the machine.

Evidence lives under `executions/run-<operation-hash>/`: bounded stdout/stderr
files, runtime trees and `report.json`. The report includes raw parsed samples,
process outcomes, output hashes, method/configuration/source identities, decisions
and reasons. After syncing the evidence, one journal event records its SHA-256,
finishes the attempt and updates the accepted reference when the verdict is
`kept`. Repeating the operation returns the same report without re-execution.
A complete report written before that event can be reconciled with the same
operation ID. Later candidates start from the accepted snapshot. Rejected and
inconclusive evidence is retained; it does not replace the accepted reference.

## Supervision and recovery

The runner launches argv directly in a new POSIX process group. It drains bounded
stdout/stderr through nonblocking streams, handles SIGINT/SIGTERM and cancellation
requests, and enforces command timeouts. Shutdown sends TERM, then KILL after
100 ms; cleanup has a two-second bound. An unreaped leader pins the group identity
until the final signal. Descendants in that group are signalled even when their
parent exits first. Buffered output must be drained before reporting success.

A durable `process.json` marker records launch/ownership. After a supervisor crash,
resume/stop never kill a recorded PID: a live or uninspectable group blocks recovery.
An absent group allows metadata recovery; a crash before the group identity was
recorded requires inspection. Resume abandons incomplete work, retaining reserved
time. Do not delete locks or process markers to force recovery while work may live.

This is trusted local execution, not a security sandbox. Processes that escape
the group, access external files/network or alter metadata under the same user
account are outside its isolation guarantees. Output is capped at the configured
combined limit, with a 64 MiB per-command ceiling. Session storage is checked
between stages and approximately every 20 ms while commands run; rapid writes can
overshoot. This is a polled limit, not an OS disk quota. Four MiB are reserved for
the final report. Evidence is retained; automatic retention cleanup is not present.

### Continue after timeout or host interruption

A completed timeout remains a failed evaluation. Retrying its original operation
returns the same report without rerunning or refunding time. Start any authorized
new work with a new operation ID and the remaining original budget. Stop/resume
changes session metadata and launches no command.

After abrupt supervisor death, `status` exposes the pending execution/reservation.
A live recorded group blocks resume; no recorded PID is killed by recovery. Once
that group has exited, explicit resume may abandon incomplete work while retaining
its full reservation. A launch interrupted before the group ID was recorded still
requires inspection, even if the child later exits. A complete durable report
should instead be reconciled by retrying its original baseline/evaluate request
before abandoning the execution. No incomplete run is promoted, and neither path
extends the deadline. Source edits lacking a sealed snapshot are not reconstructed
from a historical log.

## Budget accounting

One candidate evaluation consumes one attempt; qualification consumes no attempt.
Setup, checks, hooks, warmups, measurements and confirmation all reserve time
before launch. Each reservation is the smallest of the command timeout, remaining
active time and time until the original deadline. Settlement charges monotonic
process elapsed time, including cleanup; overruns remain charged. Snapshot copying,
hashing and idle time between commands are not included in active process time.

An interrupted or abandoned reservation keeps its full charge. Stop/resume never
restore attempts, consumed time or the original deadline. A depleted budget prevents
another launch, including confirmation; incomplete evaluation cannot promote code.
The legacy single-command `reserve_work`/`settle_work` API remains separate and
cannot reserve work during an engine evaluation. It is not exposed through the CLI.

## CLI output and exit codes

`--json` produces one JSON object on stdout using envelope `schema_version: 1`;
this is independent of configuration version 2. `schema` always emits the schema
object itself. Metadata responses contain state, remaining budgets, deadline,
clock/recovery/projection flags and `commands_executed: false`. Idle resume/stop also
report `already_applied`. Configuration values are not echoed in parse errors or
status output. Workspace responses instead include `experiments_executed: false`
and an `artifact` object with path, SHA-256, file count, retry status and
`evaluated: false`. Session state lists journaled artifact fingerprints. Text
errors go to stderr. `baseline`/`evaluate` return an `evaluation` object with the
report, evidence SHA-256 and retry status. Exit zero means the operation completed;
it does not imply `kept`. Inspect `evaluation.report.decision` (`qualified`, `kept`,
`discarded`, `inconclusive`, `failed` or `cancelled`).

| Code | Meaning |
| --- | --- |
| 0 | Requested operation succeeded, including an idempotent retry. |
| 1 | Filesystem I/O failed. |
| 2 | Invalid arguments, identifier, configuration or historical journal. |
| 3 | Unsupported session platform, or `doctor` could not find Git on a supported platform. |
| 4 | Conflicting operation, session lock busy, missing/stale baseline, or unavailable report. |
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

Execution tests also cover descendant cleanup, TERM resistance, bounded output,
stop/SIGTERM cancellation, noisy references, constraints, confirmation failure,
atomic evidence recovery and promotion followed by regression.

Both skills now include separate Rust/portable guides, with complete resources
in each host bundle. CLI acceptance covers a separate pilot, executable/config/
source/export paths containing spaces and Unicode, qualification, promotion,
regression, inspection and stop/resume without Pi or an LLM.

Historical import tests cover profiles, malformed and duplicate records, status
rejection, budgets, idempotence, original-file preservation and fresh qualification.
Recovery tests cover actual supervisor death with a surviving process group and
explicit continuation after timeout without resetting budgets.

Next comes the Pi adapter (lot 9), then full result bundles and release packaging.
See `IMPLEMENTATION_PLAN.md`.
