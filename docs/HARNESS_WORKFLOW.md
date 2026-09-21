# Using Autoresearch Toolkit with an agent

Use your harness guide for installation and skill discovery, then follow this
workflow. It applies to Codex, Claude Code, Cursor, Pi skills and generic hosts.
The optional Pi extension additionally exposes engine operations as native tools.

## Requirements and package choice

Use a local Linux or macOS project, Git, executable behavior checks, and the
project's benchmark dependencies. Portable measurements require Python 3.10+.
Rust mode requires the Toolkit CLI; Node.js is only needed for Pi. Your agent's
account, model access and permissions are configured separately. The Rust engine
and its demonstrations require no LLM provider or API key.

An archive built with `--profile skills` or an ordinary folder export contains
skills and documentation, but no engine binary. An `engine` archive adds the
matching native `bin/autoresearch`. Only `--agent pi --pi-adapter` also includes
the Pi extension. Do not run source build commands inside an extracted archive:
Cargo sources, export scripts and development tests are not included there.
See [installation and compatibility](INSTALL.md) for archive selection and hashes.

Keep the complete package and its MIT/third-party notices. When copying skills
into an agent discovery directory, copy both complete skill folders with their
references, scripts and assets. Do not copy only `SKILL.md`. Keep one discovery
method active to avoid duplicate names. Installation examples assume new
destinations; stop if an existing installation is present rather than merging
old and new files. Nothing here changes global agent settings automatically.

## Select the engine and execution permissions

From a full source checkout, build with `cargo build --workspace --locked` and
use the absolute path to `target/debug/autoresearch`. From an engine archive,
use the absolute path to `bin/autoresearch`. Verify the chosen executable:

```sh
/absolute/path/to/autoresearch doctor --json
```

Require `schema_version: 1`, `capabilities.run_experiments: true` and
`capabilities.inspect_evaluations: true`. Tell the agent that exact path. Do not
assume that an application launched from the desktop inherits your shell PATH.
An absent or incompatible engine blocks an explicitly requested Rust session;
it must not cause a silent switch to portable mode.

The agent needs read access to the source and toolkit, plus write/shell access to
the selected pilot directory and candidate workspaces. Preserve the host's
approval controls. The engine runs trusted local commands and requires
`execution.network: "allowed"`; it rejects `"disabled"` because no network
isolation backend exists. Host permissions can further restrict execution.

## Check discovery without an experiment

First use the host's skill inventory/picker to check that both
`autoresearch-scout` and `autoresearch-run` appear exactly once. Then, if you want
a model-backed check, select the scout skill and send:

> Audit only. Read this skill and its referenced workflow. Describe the selected
> mode and any missing inputs. Do not run commands, create a session, edit files,
> install dependencies or start an experiment.

A model-backed check uses your host's configured provider and may incur its usual
usage costs. A manifest validator or skill listing alone does not establish that
the model follows the workflow correctly.

## Prepare a baseline

Select `autoresearch-scout` using the invocation syntax in your harness guide:

> Use Rust mode with `/absolute/path/to/autoresearch`. Optimize the project at
> `/absolute/path/to/project`, with pilot root `/absolute/path/to/pilot`.
> Goal: reduce execution time of [real benchmark command] while [real behavior
> check] keeps passing. Allow changes only under [paths]; protect [tests and
> benchmark paths]. Budget: 5 experiments, 20 minutes total command time, and a
> deadline 30 minutes from now. Prepare and qualify the baseline only. Do not
> optimize, commit or push.

Replace bracketed values with real inputs. The scout must resolve missing scope,
metric, threshold and command information before initializing. Its
[contract template](../skills/autoresearch-scout/assets/session.json) contains
placeholders and is not a ready-to-run project contract. See the
[Rust preparation reference](../skills/autoresearch-scout/references/engine.md).
An unstable reference blocks optimization.

For portable mode, replace the first sentence with “Use portable mode in an
isolated experimental checkout.” Scope and session budgets are then enforced by
agent instructions; the measurement helper enforces each invocation's limits.
See [portable preparation](../skills/autoresearch-scout/references/portable.md).

## Run and inspect results

After reviewing the baseline handoff, select `autoresearch-run`:

> Optimize using the prepared session, its existing scope and remaining budget.
> Test one hypothesis at a time. Edit only the returned candidate workspace.
> Keep protected tests unchanged. Report each engine verdict and its evidence.
> Do not commit, push, or overwrite the source project.

Rust promotion changes the accepted experimental reference, not your source
checkout. A completed command can report `discarded`, `inconclusive`, `failed`
or `cancelled`; exit zero is not proof of improvement. Inspect evidence directly:

```sh
/absolute/path/to/autoresearch history --root /absolute/path/to/pilot \
  --session SESSION_ID --json
/absolute/path/to/autoresearch report --root /absolute/path/to/pilot \
  --session SESSION_ID --json
```

The default report describes the accepted reference. To inspect a specific trial,
use its evaluation key from `history`. Ask explicitly for a code or result export
and choose a new output directory. Review it before applying changes to your
project. [Result bundles](RESULTS.md) explains code, logs and privacy selections.

## Stop, resume and recover

Ask the agent to stop and use the host's interruption control. For a Rust session,
request cancellation from another terminal when needed:

```sh
/absolute/path/to/autoresearch stop --root /absolute/path/to/pilot \
  --session SESSION_ID --operation-id stop-001 --json
/absolute/path/to/autoresearch status --root /absolute/path/to/pilot \
  --session SESSION_ID --json
```

Check that execution has settled before restarting. `resume` restores metadata;
it does not run another experiment or reset budgets/deadlines. A new optimization
request is still needed. Use a new operation ID for a new mutation; reuse the
original ID only to retry that same request. After a crash, follow the
[engine recovery guide](RUST_ENGINE.md); never delete locks or journals just to
make a session appear runnable. Portable sessions use the host's interrupt and
must retain their recorded scope, evidence and remaining budget.

## Upgrade, uninstall and troubleshooting

Stop active work first. Install a new package into a separate directory, back up
sessions, then update the selected skill/binary paths. Do not assume older
binaries can read sessions advanced by a newer engine. Remove only the Toolkit
skill copies or registration you installed; retain `.auto/` evidence and source
repositories. The harness guide specifies its registration mechanism.

| Symptom | Check |
| --- | --- |
| No skills in the picker | Correct project, complete skill directories, host discovery/reload and trust settings |
| Two copies of a skill | Remove one registration method; check project and user installations |
| Script or reference not found | Restore the entire skill folder, preserving relative paths |
| Engine missing or incompatible | Check the absolute path and `doctor --json`; do not change session mode |
| Command denied | Review host permissions and contract paths; do not disable protections globally |
| Baseline unstable or no gain | Inspect the recorded measurements; do not weaken tests to force acceptance |
| Session busy or interrupted | Inspect `status` and the recovery procedure before any new mutation |

## What has been validated

The RC1 qualification at commit `5f0cd3b` passed 20 CI jobs: native Linux/macOS
ARM64 and Intel packages, Python/Rust suites, and Pi integration/loading. See
[qualification results](INSTALL.md#native-target-matrix) for the exact revision.
New documentation/package revisions must pass their own checks.

Harness-specific checks and remaining interactive checks are stated in each
harness guide. Package integrity, native skill discovery, manifest validation,
and a model-backed end-to-end session are separate levels of evidence. No
universal host version compatibility or marketplace publication is implied.

Local harness checks on 2026-09-21:

| Harness | Evidence | Still to check |
| --- | --- | --- |
| Codex 0.154.0 | Native `skills/list` finds both project skills enabled | Interactive picker and model-backed workflow |
| Claude Code 2.1.278 | Strict validation of the exported plugin manifest succeeds | Interactive plugin discovery and model-backed workflow |
| Cursor 3.19.13 | Installed version inspected; procedures checked against official docs | Interactive discovery and model-backed workflow |
| Pi 0.85.1 | Both exported skills load without diagnostics; extension integration and packaged loading covered separately | User's chosen model/provider workflow |
| Generic format | Package inventory and resource links checked | Actual host/version integration |
