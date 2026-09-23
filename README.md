# Autoresearch Toolkit

Created and maintained by **Metimer**.

A Rust engine and agent skills for measured optimization: establish a baseline,
test one hypothesis at a time, and keep only verified improvements. The engine
also runs directly from the CLI without Pi or an LLM.

**1.0.0-rc.1** is available as a locally buildable candidate. See the
[installation and compatibility guide](docs/INSTALL.md) and
[release notes](docs/RELEASE_NOTES.md). The
[qualification matrix](docs/INSTALL.md#native-target-matrix) links CI results and
artifacts by source revision. Stable publication is a separate step.

The workflow has two stages:

| Skill | Purpose |
| --- | --- |
| `autoresearch-scout` | Discover tests and benchmarks, define the scope, and establish a reproducible baseline. |
| `autoresearch-run` | Run experiments within a defined budget, compare results, and retain validated changes. |

## Choose a mode

| Mode | Execution and evidence | Requirements |
| --- | --- | --- |
| Rust | The engine owns isolated candidates, command budgets, checks, measurements and acceptance. | An independently installed/built `autoresearch` binary, Git, Linux or macOS, and the project's command dependencies. |
| Portable | The agent drives the loop and enforces scope/session budgets; a Python helper bounds each measurement invocation. | An agent with file/shell tools, Git and Python 3.10+ on Linux/macOS. |

Both modes need a target project with executable behavior checks. Windows and WSL
are not qualified targets for this candidate. New skill sessions prefer a compatible engine when available; explicitly
request portable mode if desired. The selected mode is announced. Existing
sessions keep their mode: an absent/incompatible Rust binary blocks a Rust session
without downloading anything or silently falling back to portable execution.

## Quick start with an agent

### 1. Prepare a baseline

In a session opened on the project you want to optimize:

> Read `/path/to/autoresearch-toolkit/skills/autoresearch-scout/SKILL.md`.
> Prepare a performance baseline without optimizing or committing.
> Goal: reduce the execution time of [command or workload].
> Session ceiling: 5 experiments, 20 minutes of command time and a deadline
> 30 minutes from now. Spend at most 10 minutes on preparation.

The scout discovers real commands, defines the metric and scope, then qualifies
the reference within those limits. A Rust session stores its immutable contract
and evidence under the selected pilot root's `.auto/engine/sessions/`. A portable
session stores `.auto/prompt.md`, `.auto/context.md` and baseline evidence in the
experimental checkout. An unstable baseline blocks optimization.

### 2. Run experiments

Once the reference is qualified:

> Read `/path/to/autoresearch-toolkit/skills/autoresearch-run/SKILL.md`.
> Optimize using the prepared session, its recorded scope and remaining limits.
> Test one hypothesis at a time. Do not commit.

Rust creates private candidates from the accepted reference, evaluates sealed
code and promotes only confirmed improvements. The portable mode requires an
isolated experimental checkout and records results in `.auto/portable-log.jsonl`
and retained edits in `.auto/portable-state.md`.

Preparation alone does not authorize optimization. Starting or resuming a run
preserves the original deadline and remaining budget. Adjust paths and limits to
the workload; `.auto/` evidence stays local. Neither mode creates source commits
unless separately requested.

## Integration formats

The toolkit provides the following manifests, all using the same skills:

| Host | Installation and usage | Included manifest |
| --- | --- | --- |
| Codex | [Codex guide](docs/harnesses/codex/README.md) | `.codex-plugin/plugin.json` |
| Claude Code | [Claude Code guide](docs/harnesses/claude/README.md) | `.claude-plugin/plugin.json` |
| Cursor | [Cursor guide](docs/harnesses/cursor/README.md) | `.cursor-plugin/plugin.json` |
| Pi | [Pi guide](docs/harnesses/pi/README.md) | `package.json`, through `pi.skills` |
| Agent Plugins | [Generic host guide](docs/harnesses/generic/README.md) | `plugin.json` |

Use your host's loading mechanism or give the agent the skill's path directly,
as shown above. Choose one discovery method to avoid duplicates. Providing a
manifest does not guarantee that every version of the host can load it.
Each guide covers installation, invocation, updates, removal and troubleshooting,
with its actual validation level. The [shared workflow](docs/HARNESS_WORKFLOW.md)
provides engine selection, complete prompts, stop/resume and result inspection.
Each export includes its selected guide and the shared documentation.

## Export a bundle

Build a versioned archive with an explicit profile:

```sh
python3 scripts/release.py --profile skills --agent codex --output dist/skills-codex
python3 scripts/release.py --profile engine --agent pi --pi-adapter --output dist/engine-pi
python3 scripts/qualify.py --archive dist/engine-pi/ARCHIVE_NAME.tar.gz \
  --output dist/engine-pi/qualification.json
```

Replace `ARCHIVE_NAME` with the filename printed by the builder. Engine archives
include the native CLI, dependency notices, and two reproducible demonstrations.
The optional Pi adapter is selected explicitly. Every archive has a file inventory
and checksum; building and qualifying it does not publish anything. See the
[installation guide](docs/INSTALL.md) for platform requirements and Pi setup.

The original skills-only folder export remains available:

From the repository root, create a bundle for your chosen host:

```sh
python3 scripts/export.py --agent codex --output dist/codex/autoresearch-toolkit
```

Accepted values for `--agent` are `codex`, `claude`, `cursor`, `pi`, and `generic`.
The destination must be a new directory named `autoresearch-toolkit`, located
outside the source skills directory.

Each bundle contains the skills, their resources, the selected manifest, the
README, and the license. The bundle includes both mode guides and a Rust contract
template, but no Rust binary, source build or automatic installer. The exporter uses an explicit file list and rejects
symbolic links in source paths. It does not overwrite an existing installation.
If copying fails, a partial output may remain on disk; inspect it before retrying.

## Measure a command

You can also use the measurement helper directly:

```sh
python3 skills/autoresearch-scout/scripts/measure.py \
  --name bench_ms --runs 5 --warmup 1 --timeout 10 --budget 60 \
  -- python3 -c 'sum(range(1000000))'
```

It measures elapsed time in milliseconds and prints three `METRIC` lines: the
median (`bench_ms` in this example), minimum (`run_min_ms`), and maximum
(`run_max_ms`). Warmup measurements are excluded. Command output goes to stderr;
a failure or timeout produces no metrics.

The helper measures command duration. Process startup adds noise to very short
workloads. Memory, size, and throughput require a suitable measurement command.

## Execution model

In Rust mode, the engine supervises trusted commands, enforces its process budgets
and evaluates immutable candidate snapshots. The agent proposes and edits each
candidate; it does not assign the acceptance verdict. `history` and `report`
read engine-owned evidence without rerunning experiments.

In portable mode, editing scope, test protection and the overall budget are
instructions for the agent. The measurement helper enforces its invocation's
timeouts and terminates its owned process group on timeout or interruption.

Neither mode is a security sandbox. Rust currently requires `network: "allowed"`
for execution; it rejects `"disabled"` because no network isolation backend exists.
No API key or LLM provider is needed by the engine or toolkit scripts. Project
commands may have their own dependencies. Resuming a loop needs a new invocation.

## Import historical journals

Pi and portable JSONL journals can be retained in a new Rust session as unverified
history. Inspect the source before importing:

```sh
autoresearch inspect-legacy --source /path/to/old-log.jsonl --json
autoresearch import-legacy --source /path/to/old-log.jsonl \
  --config /path/to/new-session.json --root /path/to/pilot \
  --operation-id import-001 --json
autoresearch report --session imported-session --root /path/to/pilot --legacy --json
```

Use the new configuration's session ID in the last command. The source stays
unchanged; import requires a new session ID and explicit version 2 contract and
budgets. Unknown statuses, malformed records and unsupported versions block import.
No old script, verdict or uncommitted edit is resumed. Capture source and qualify
it again before optimization; historical `keep` never means engine `kept`.

The [engine guide](docs/RUST_ENGINE.md#historical-import) documents supported
profiles, limits and recovery after interruption. Ordinary Rust sessions retain
their existing budgets through `resume`; import is a separate operation.

## Development

### Rust engine

An agent-independent Rust engine provides strict configuration validation and
persistent sessions with a journal, exclusive writer locks and budget accounting
across restarts. It can also create independent Git snapshots, prepare and seal
scoped candidates, supervise commands, qualify a reference and compare candidates
with a separate confirmation series. Verified improvements become the next
reference; patches can be exported without committing to the source repository.
An optional [Pi adapter](adapters/pi/README.md) connects Pi 0.85.1 to an explicit
Rust session, including cancellation and recorded verdicts. Both skills support
the Rust and portable workflows; the root Pi manifest loads those skills only.

From a full source checkout, with Rust 1.81 or later:

```sh
cargo build --workspace --locked
./target/debug/autoresearch doctor --json
./target/debug/autoresearch validate --config examples/session.json --json
```

The example contains placeholders. Validation checks structure and consistency;
it does not verify the repository or execute the declared commands. The Rust
binary runs without Node.js or a Pi installation. Building it may download the
dependencies recorded in `Cargo.lock`.

To initialize and inspect metadata in an existing project directory:

```sh
./target/debug/autoresearch init --config examples/session.json \
  --root /path/to/project --operation-id create-001 --json
./target/debug/autoresearch status --session example-session \
  --root /path/to/project --json
```

`resume` restores session metadata without launching commands. `stop` requests
cancellation when an evaluation is running, or records a stopped session otherwise.
Repeating a mutation with the same operation ID does not repeat
its effects; session budgets and the original deadline survive reopening.
`autoresearch schema` prints the version 2 configuration schema.

After replacing the example's placeholders with a real local repository and commit:

```sh
./target/debug/autoresearch workspace --session example-session \
  --root /path/to/project --local-changes exclude --operation-id workspace-001 --json
./target/debug/autoresearch prepare-candidate --session example-session \
  --root /path/to/project --candidate trial-001 --hypothesis "Reduce repeated work" \
  --operation-id prepare-001 --json
```

For execution, set `execution.network` to `"allowed"` only for trusted local
commands, and configure their environment and generated paths explicitly. The
current runner rejects `"disabled"` because it has no network isolation backend.
Your benchmark must emit one JSON line per declared metric, for example:

```text
METRIC {"name":"bench_ms","value":12.4,"unit":"ms"}
```

This strict engine protocol differs from the portable measurement helper's output.
Qualify the reference, edit the returned candidate directory, seal it, then evaluate:

```sh
./target/debug/autoresearch baseline --session example-session \
  --root /path/to/project --operation-id baseline-001 --json
./target/debug/autoresearch seal --session example-session \
  --root /path/to/project --candidate trial-001 --operation-id seal-001 --json
./target/debug/autoresearch evaluate --session example-session \
  --root /path/to/project --candidate trial-001 --operation-id evaluate-001 --json
```

Read `evaluation.report.decision`: `kept` requires passing checks, sufficient gain
and a distinct confirmation series. A completed evaluation can also be `discarded`,
`inconclusive`, `failed` or `cancelled`. Exit zero means the operation completed,
so automation must inspect the decision. Use `export-candidate` as described in
the engine guide to export code; evaluation evidence remains in the session.
`--local-changes exclude` starts from the declared commit; `include` explicitly captures current tracked and non-ignored untracked
files. A code export alone does not certify that a candidate passed evaluation.

Inspect completed results without launching commands:

```sh
./target/debug/autoresearch history --session example-session \
  --root /path/to/project --json
./target/debug/autoresearch report --session example-session \
  --root /path/to/project --json
```

`report` defaults to the qualified accepted reference. Use `--evaluation` with a
key from `history` to inspect any completed result, including rejected trials.
`status` includes an advisory `next_action`; it does not authorize an experiment.

Search native evaluations across sessions and preview a shareable report:

```sh
autoresearch memory --root /path/to/project --query 'src/' --json
autoresearch result-preview --root /path/to/project --session example-session \
  --evaluation run-REPLACE_WITH_KEY --json
```

`export-result` publishes the selected report to a new directory. Code, command/
environment declarations and individual raw logs require explicit selection;
the default report excludes them. See [memory and result bundles](docs/RESULTS.md)
for export, reproduction, privacy and exact duplicate detection.

See [Rust engine development](docs/RUST_ENGINE.md) for the configuration contract,
commands, recovery procedure and storage guarantees. Skills-only bundles
do not include the Rust sources or binary; engine archives include the native CLI.

### Checks

Run the tests from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 -m unittest discover -s tests -v
```

Characterization tests for the archived JSONL reader additionally require Node.js 24:

```sh
node --test tests/legacy/jsonl.test.mjs
```

The optional Pi adapter has its own pinned dependencies and integration tests:

```sh
cargo build --locked
npm --prefix adapters/pi ci --ignore-scripts --no-audit --no-fund
npm --prefix adapters/pi run check
npm --prefix adapters/pi test
```

The suite covers measurements, failures, timeouts, and exports for all five
formats. When adding a resource to a skill, declare it in `PORTABLE_FILES` in
`scripts/export.py` to include it in distributed bundles.

Reference sources in `originals/` are separate from the maintained skills. They
are neither loaded by the root manifests nor included in exports.

## License

[MIT](LICENSE). Third-party components retained in `originals/` keep their
respective licenses and attributions. See [third-party notices](THIRD_PARTY_NOTICES.md).
