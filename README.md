# Autoresearch Toolkit

Skills that guide a coding agent through measured optimization: establish a
baseline, test one hypothesis at a time, and keep only verified improvements.

The workflow has two stages:

| Skill | Purpose |
| --- | --- |
| `autoresearch-scout` | Discover tests and benchmarks, define the scope, and establish a reproducible baseline. |
| `autoresearch-run` | Run experiments within a defined budget, compare results, and retain validated changes. |

## Requirements

- An agent that can read files, edit code, and run commands.
- A target project under Git with executable behavior checks.
- Python 3.10 or later for the toolkit scripts, with no third-party dependencies.
- macOS or Linux for the measurement helper; use WSL on Windows.

## Quick start

### 1. Prepare a baseline

In an agent session opened on the project you want to optimize:

> Read `/path/to/autoresearch-toolkit/skills/autoresearch-scout/SKILL.md`.
> Prepare a performance baseline for this repository without optimizing or committing.
> Goal: reduce the execution time of [command or workload].
> Preparation budget: 10 minutes.

The scout identifies the project's actual commands, defines the metric and allowed
files, then runs the checks and several rounds of measurements. It stores the
context, methodology, and results in the target project's `.auto/` directory.
An unstable baseline must be improved before experiments begin.

### 2. Run experiments

Once the baseline is validated, use an isolated checkout for experiments:

> Read `/path/to/autoresearch-toolkit/skills/autoresearch-run/SKILL.md`.
> Use the `.auto/` session prepared for this repository.
> Run at most 5 experiments within 20 minutes, changing only the paths allowed
> in `.auto/prompt.md`. Do not commit.

The agent states a hypothesis, makes a scoped change, runs the tests, and measures
its effect. An improvement must exceed the observed noise and preserve the
project's behavior. Results are recorded in `.auto/portable-log.jsonl`; retained
changes are described in `.auto/portable-state.md` for an explicit resume request.

Adjust paths and budgets to your installation and project. Preparing a baseline
and starting experiments are separate requests. The `.auto/` files stay local
to the target project.

## Integration formats

The toolkit provides the following manifests, all using the same skills:

| Host | Included manifest |
| --- | --- |
| Codex | `.codex-plugin/plugin.json` |
| Claude Code | `.claude-plugin/plugin.json` |
| Cursor | `.cursor-plugin/plugin.json` |
| Pi | `package.json`, through `pi.skills` |
| Agent Plugins | `plugin.json` |

Use your host's loading mechanism or give the agent the skill's path directly,
as shown above. Choose one discovery method to avoid duplicates. Providing a
manifest does not guarantee that every version of the host can load it.

## Export a bundle

From the repository root, create a bundle for your chosen host:

```sh
python3 scripts/export.py --agent codex --output dist/codex/autoresearch-toolkit
```

Accepted values for `--agent` are `codex`, `claude`, `cursor`, `pi`, and `generic`.
The destination must be a new directory named `autoresearch-toolkit`, located
outside the source skills directory.

Each bundle contains the skills, their resources, the selected manifest, the
README, and the license. The exporter uses an explicit file list and rejects
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

The agent drives the workflow. Editing scope, test protection, and the overall
budget are instructions it must follow. The helper enforces its own measurement
timeouts and terminates the process group it started on timeout or interruption;
it is not a sandbox.

The toolkit scripts require no API key or LLM provider connection. The agent and
the project's commands may have their own dependencies and network requirements.
Resuming the loop requires a new invocation.

## Development

Run the tests from the repository root:

```sh
python3 -m unittest discover -s tests -v
```

The suite covers measurements, failures, timeouts, and exports for all five
formats. When adding a resource to a skill, declare it in `PORTABLE_FILES` in
`scripts/export.py` to include it in distributed bundles.

Reference sources in `originals/` are separate from the maintained skills. They
are neither loaded by the root manifests nor included in exports.

## License

[MIT](LICENSE). Third-party components retained in `originals/` keep their
respective licenses and attributions.
