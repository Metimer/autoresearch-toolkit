---
name: autoresearch-scout
description: Pre-flight for pi-autoresearch on an existing codebase. Maps the repo (stack, test/lint/build commands, hot paths), proposes measurable optimization metrics, writes .auto/measure.sh, .auto/checks.sh and .auto/prompt.md, validates the baseline, then hands off to /autoresearch. Use when asked to "prepare autoresearch", "set up an optimization loop on this repo", "scout this codebase for autoresearch", or before running /autoresearch on a repo that has no .auto/ folder yet.
---

# Autoresearch Scout

You prepare a repository for an autonomous optimization loop run by `pi-autoresearch`.
You do NOT run experiments. You produce the three files the loop needs, prove the
baseline is measurable and green, then stop and tell the user to run `/autoresearch`.

Work through the phases in order. Do not skip a phase. Do not start optimizing.

## Phase 0 — Preconditions (bash)

1. `git rev-parse --is-inside-work-tree` must succeed. If not, stop and tell the user.
2. `git status --porcelain` must be empty. If not, stop and ask the user to commit or stash.
3. If `.auto/prompt.md` already exists, stop: tell the user to run `/autoresearch` to resume,
   or `/autoresearch clear` first.
4. Record the current branch. Create the working branch now:
   `git checkout -b autoresearch/<short-goal>-$(date +%Y%m%d)`
   (use `scout` as short-goal if the goal is not known yet).

## Phase 1 — Repo map (delegate to `scout`)

Use the `subagent` tool with `agent: "scout"` and the task below, verbatim, replacing `<GOAL>`
with the user's goal or "not specified yet". If the `subagent` tool is unavailable, do the
same recon yourself with `find`, `grep`, `read`, staying under 15 tool calls.

```
Map this repository for an automated optimization loop. Goal: <GOAL>.
Report, tersely, in this exact structure:
1. STACK: languages, package manager, framework, monorepo layout (yes/no, workspaces).
2. COMMANDS: the exact commands found in package.json scripts, Makefile, pyproject/tox/noxfile,
   Cargo.toml, go.mod, CI workflows (.github/workflows, .gitlab-ci.yml) for: test, lint,
   typecheck, build, bench. Quote the file each command comes from. Say "none" when absent.
3. TEST RUNNER: name, how to run it non-interactively with minimal output, approximate
   test count, approximate wall time if CI logs or config reveal it.
4. EXISTING BENCHMARKS: any bench/, benchmarks/, perf/, *.bench.* files and how to run them.
5. ENTRY POINTS AND HOT PATHS: main entry files, the 5-10 largest or most imported source
   files, anything obviously performance-relevant (loops over data, I/O, serialization,
   heavy dependencies, build config).
6. SIZE: number of source files, total lines, size of build output directory if present.
7. RISKS: generated files, vendored code, files that must not be edited, flaky tests noted
   in comments or CI config, missing lockfile.
Do not modify anything. Do not propose optimizations.
```

Read the scout's `context.md` output. Keep it; you will cite it in `.auto/prompt.md`.

## Phase 2 — Choose the metric

Build a short candidate table from the map. Typical candidates, in order of preference:

| Candidate | When it applies | Measure with |
|---|---|---|
| `test_time_ms` | test suite exists and runs < 5 min | wall time of the test command |
| `build_time_ms` | build/compile step exists | wall time of the build command |
| `bundle_bytes` | web bundle or binary artefact produced | `du -sb` / `stat -f%z` on the artefact |
| `bench_ms` | existing benchmark script | its own output, or wall time |
| `startup_ms` | CLI or server | time to first output / first healthy request |
| `peak_rss_kb` | memory matters | `/usr/bin/time -l` (macOS) or `-v` (Linux) |

Rules:
- If the user gave a goal, map it to ONE primary metric and up to 2 secondary ones.
- If the user gave no goal, ask ONE question listing 2-3 candidates with the estimated
  run duration of each. Wait for the answer. Do not ask anything else.
- Reject any metric whose single run exceeds 10 minutes. Suggest a subset (one package,
  one test directory, a smaller input) instead.
- Never pick a metric the agent could improve by deleting tests, skipping cases, or
  lowering quality settings without `checks.sh` noticing. If that risk exists, add a
  secondary metric that guards it (for example `test_count` next to `test_time_ms`).

## Phase 3 — Write `.auto/measure.sh`

Start from `templates/measure.sh` in this skill's directory. Fill in `CMD`, `METRIC_NAME`,
`RUNS`, `WARMUP`, and the optional secondary metrics block. Requirements:

- `set -euo pipefail`, executable (`chmod +x`).
- Pre-check in under 1 second (syntax check, file exists, dependency installed) before the
  expensive part, so broken edits fail fast.
- Runs the workload `RUNS` times after `WARMUP` warm-ups and prints the MEDIAN as
  `METRIC <name>=<number>`. Use RUNS=5 when a run is under 10 s, RUNS=3 under 60 s,
  RUNS=1 above that.
- Prints only `METRIC` lines and short diagnostics to stdout. Pipe workload output to
  `/dev/null` or a log file under `.auto/`.
- Must never modify tracked files. If the workload writes build output, point it at a
  temporary directory or clean up after.

Then run it 3 times in a row with bash. Compute spread = (max - min) / median.
- spread <= 0.05: good.
- spread <= 0.15: acceptable; raise `RUNS` by 2 and note the noise level in `prompt.md`.
- spread > 0.15: the metric is unusable as is. Increase `RUNS`, pin the input, or pick
  another candidate. Do not proceed with a noisy metric.

## Phase 4 — Write `.auto/checks.sh` (always)

Start from `templates/checks.sh`. Always create this file, even if the user did not ask
for it: the loop is not allowed to keep a change that breaks behaviour.

- Use the exact commands the scout found (tests, typecheck, lint). Prefer the CI ones.
- Every command must run non-interactively, with `CI=1` when relevant, and quiet flags.
- Filter output to errors only (`grep -iE "error|fail|✗"` with `|| true` where needed) and
  keep the last 80 lines meaningful. The loop only sees the tail on failure.
- If the primary metric is the test suite itself, `checks.sh` must still run the tests
  (a fast subset is acceptable) plus typecheck/lint, and must assert the test count did not
  drop: capture the count and compare it to the baseline number stored in
  `.auto/baseline_test_count`.

Run it once. It must exit 0 on the untouched baseline. If it does not, stop and report the
failure to the user; never start a loop on a red baseline.

## Phase 5 — Write `.auto/prompt.md`

Start from `templates/prompt.md`. Fill every section. Be concrete: file paths, numbers,
commands. The "Off Limits" list must include at minimum:

- `.auto/` itself
- every test file and test fixture
- `checks.sh`-related config (lint, tsconfig, pytest.ini, CI workflows)
- lockfiles and dependency manifests unless the goal explicitly allows dependency changes
- generated and vendored directories found by the scout

Add the baseline numbers from Phase 3 and the noise spread. Paste the scout's hot-path list
under "Where to look first".

## Phase 6 — Commit and hand off

1. Add `.auto/log.jsonl` to `.gitignore` if not present. Do not ignore the rest of `.auto/`.
2. `git add .auto .gitignore && git commit -m "autoresearch: scout setup for <goal>"`
3. Print a short summary: branch name, primary metric with baseline and spread, secondary
   metrics, what `checks.sh` runs and how long it took, the Off Limits list.
4. End with exactly this instruction to the user, filled in:

   `Run: /autoresearch <goal in one line>`

Do not call `init_experiment`, `run_experiment` or `log_experiment` yourself. Do not edit
source files. Your job ends at the handoff.
