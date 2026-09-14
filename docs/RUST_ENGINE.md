# Rust engine development

The engine is being built as two Cargo workspace members:

- `autoresearch-core`: contracts and validation, independent of agents and CLI I/O.
- `autoresearch-cli`: the `autoresearch` binary, with text and versioned JSON output.

The workspace uses Rust 2021 with a minimum supported Rust version of 1.81.
`Cargo.lock` is committed. The initial build was tested offline against dependencies
already cached locally; the first build elsewhere may download the locked crates.
Neither crate is enabled for registry publication yet.

## Available now

```sh
cargo build --workspace --locked
./target/debug/autoresearch doctor --json
./target/debug/autoresearch validate --config examples/session.json --json
```

`doctor` reports the compiled OS and architecture, whether a Git executable can
be found on PATH, and implemented capabilities. It does not execute Git or verify
its version. Its success describes these checks only, not readiness to run an experiment.

`validate` checks JSON structure and semantic consistency. It does not open the
declared repository, run checks or benchmarks, create a session, or compare the
deadline with the current time. The core exposes a separate explicit deadline
check for the future orchestrator.

The example is a template: replace the all-zero commit, repository, commands,
scope, metric and deadline with observed values. A structurally valid placeholder
does not establish that the repository or its commands exist.

Run `autoresearch --help` for the implemented commands. Execution, persistence,
isolation, result acceptance and the Pi adapter are not implemented in this first
slice. The existing Python skills remain the operational experiment workflow.

## Draft contract

The configuration uses `schema_version: 1`. The format is an initial, unpublished
contract, not yet the complete `SessionConfig` described in the implementation plan.
Raw deserialization is followed by semantic validation to construct a read-only
`ValidatedConfig`. Both unknown and duplicate fields are rejected. Unknown outcomes
are errors; they never default to `kept`.

| Field | Rule |
| --- | --- |
| `session_id` | 1–64 ASCII letters, digits, hyphens or underscores. |
| `source` | Repository path and a full 40- or 64-character hexadecimal commit ID; existence is not checked here. |
| `scope` | Literal relative paths; a trailing slash means a directory prefix. No globs, traversal or reserved `.git`/`.auto` scope. Protected subdirectories may narrow an allowed directory. |
| `checks` | At least one declared behavior-check command. Declarations do not prove checks are effective. |
| `benchmark` | An executable, explicit argument list and workspace-relative cwd; no implicit shell. |
| `metric` | Name, unit, direction, domain and a finite positive minimum useful improvement in that unit. |
| `sampling` | 3–31 measured samples and 0–31 warmups. The comparison protocol remains to be implemented. |
| `budget` | Positive attempt, time and byte limits; per-command timeout cannot exceed active time, and output cannot exceed artifact storage. |

Inputs are limited to 1 MiB. Paths are checked lexically, without inspecting the
filesystem. Symlink resolution, case collisions and repository identity belong
to the workspace implementation. A source path is resolved later against an
explicit invocation context; it is not interpreted by the contract parser.

Still to add to the versioned contract: environment and cache policy, protected
content hashes, setup steps, secondary constraints, hooks, persistence events,
generated-output policy, schema generation and schema/runtime parity tests.
The minimum contract must not authorize experiment execution until these gates
are implemented. Breaking changes require explicit versioning before distribution.

## CLI output and exit codes

`--json` returns one JSON object on stdout. Successful validation reports
`repository_verified: false`, `deadline_checked: false` and
`commands_executed: false`. It does not print the submitted configuration or its
values in parsing errors. Text errors go to stderr.

| Code | Meaning |
| --- | --- |
| 0 | Requested read-only operation succeeded. |
| 1 | Configuration file I/O failed or the input is not a regular file. |
| 2 | Invalid arguments or invalid configuration. |
| 3 | `doctor` found an unsupported OS or no Git executable on PATH. |

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 -m unittest discover -s tests -v
node --test tests/legacy/jsonl.test.mjs
```

Node.js 24 is needed only for the characterization tests of the archived TypeScript
reader. The Rust library and CLI need neither Node.js nor a Pi installation.
The CI workflow defines Linux/macOS jobs for Rust 1.81 and stable, Python 3.10/3.13,
and a separate Node 24 characterization job. Defining that matrix does not mean
it has already run successfully on a remote service.

## Migration boundaries

| Archived behavior | Rust policy |
| --- | --- |
| Unknown/missing status becomes `keep`. | Reject the outcome; imported data will not become verified results. |
| Malformed JSONL lines are skipped, even in the middle. | Future journal recovery must distinguish truncated tails from corruption. |
| New configuration after results starts a segment. | Preserve historical segmentation during the future explicit import. |
| Secondary units are inferred from names. | Require units in new contracts; preserve historical values as historical evidence. |
| Agent submits the verdict and metric. | Future acceptance must bind engine-produced evidence to a sealed candidate. |

The archive remains unchanged. No legacy reader code has been copied into the Rust
crates. Attribution is recorded in `THIRD_PARTY_NOTICES.md`.

## Next implementation steps

Complete the lot 1 contract and schema tests, then implement the lot 2 journal,
state transitions, locks and budget accounting. Repository isolation and process
execution follow after the persistence and policy contracts are stable.
See `IMPLEMENTATION_PLAN.md` for the complete dependency order.
