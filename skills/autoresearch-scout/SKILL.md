---
name: autoresearch-scout
description: Prepare a repository for measured optimization (autoresearch). Discover real checks and benchmarks, define a protected workload and establish a stable baseline. Does not optimize code or start an autonomous loop.
---

# Autoresearch Scout

Prepare an agent-independent session; stop at the handoff. All paths below refer
to the target repository unless explicitly relative to this skill.

## Inspect before writing

Read applicable repository instructions. Record Git HEAD, branch and dirty state.
Map stack, exact test/lint/typecheck/build commands from manifests and CI, existing
benchmarks, plausible hot paths and protected files. Use rg and file reads; do not
invent commands or test counts. Delegate only if authorized and available.

If the request is only an audit, produce the map without session writes or tests.
For preparation, agree on ONE primary metric, direction, workload, allowed paths,
and compute/time budget. If these cannot be inferred, ask the missing question.
Do not install dependencies, use external services, commit, stash or switch branches
merely because this skill was selected.

Preserve unrelated changes. Before experiments, prefer a separate authorized
worktree; explain that uncommitted changes are not included in its HEAD. Do not
start a loop in a shared dirty checkout. Existing .auto/ means inspect/resume,
not overwrite or clear. Reject a symlinked .auto/ or session file.

## Prepare a reproducible session

Keep session state in .auto/. Respect existing ignore policy. If needed and within
the requested setup, add /.auto/ to .gitignore (or a local Git exclude if preferred).
Never force-add ignored files or commit setup automatically. If .auto/ is already
tracked, explain that ignoring it does not untrack it; obtain direction.

Write .auto/context.md with command provenance and risks. Adapt
[assets/prompt.md](assets/prompt.md) into .auto/prompt.md.
Create .auto/measure.sh and .auto/checks.sh with bash strict mode. Do not leave
placeholder checks that exit successfully.

For wall-time metrics, copy this skill's [scripts/measure.py](scripts/measure.py)
to .auto/measure.py and call it from measure.sh using explicit arguments:
python3 .auto/measure.py --name bench_ms --runs 5 --warmup 1 --timeout 60 --budget 360 -- COMMAND ARG...
Run from the repository root; redirect diagnostics to a session log if needed.
Python 3.10+ and a POSIX host are required for this helper. It times commands;
it does not interpret benchmark-reported metrics. For throughput/size/memory,
write an appropriate measure.sh that reports a finite METRIC name=number instead.

Use 5 runs for short workloads, 3 for medium workloads. Cap each run at 10 minutes;
fit the entire baseline within the agreed budget. Pin inputs and cache policy.
Do not delete shared caches. Separate build output into disposable owned paths.
Record environment, warmups, versions and workload identity. Measurement/check
commands must not change tracked source or external systems.

checks.sh must fail on any failing check, propagate exit status, and actually run
the applicable behavior checks. Do not hide failures with output filters or || true.
When measuring tests, protect test identities/counts AND assertions/configuration:
counts alone cannot prevent cheating. Do not initialize a missing count guard
during an experiment. Freeze baseline invariants now.

## Qualify baseline and hand off

Run checks once and measure three independent rounds, within budget. Compute
(max - min) / abs(median). A zero median is unsuitable for wall-time measurement.
Spread <=5% is good; 5–15% requires more samples within budget and requalification;
>15% blocks timing-based optimization until the metric is improved. Store raw
measurements, checks outcome, Git identity and noise. Inspect the diff afterwards:
report unexpected writes rather than reverting someone else's files.

Protect tests, fixtures, benchmark inputs, checks/configuration, dependency manifests,
lockfiles and generated/vendor directories unless the goal explicitly requires
a separately agreed change. Keep session methodology immutable during the loop;
only result logs/state may be appended.

Report baseline, noise, allowed paths, protected paths, estimated iteration cost
and remaining uncertainties. Preparation does not authorize optimization. Ask the
user to start an autoresearch run with an iteration/time budget; do not call Pi tools
or /autoresearch automatically. The portable runner consumes this session format.
