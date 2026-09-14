# Autoresearch: <goal in one line>

## Objective
<What is being optimized, for which workload, and why it matters. 3-5 sentences.>

## Metrics
- **Primary**: `<name>` (<unit>, lower is better) — baseline <value>, noise spread <x%> over 3 runs
- **Secondary**: `<name>` (<what it guards>), `<name>` (<tradeoff to monitor>)

## How to Run
`./.auto/measure.sh` — prints `METRIC name=number` lines. Takes about <N> seconds.
`./.auto/checks.sh` — runs <tests / typecheck / lint>. Takes about <N> seconds. Must stay green.

## Repo Map (from scout)
- Stack: <languages, package manager, framework>
- Test runner: <name>, <count> tests, command `<cmd>`
- Build: `<cmd>` → <artefact>
- Size: <files> source files, <lines> lines

## Where to Look First
<Hot paths and heavy files from the scout, one per line, with a short reason.>

## Files in Scope
<Every file or directory the agent may modify, with a one-line note on what it does.>

## Off Limits
- `.auto/` (this session's files)
- <test files and fixtures>
- <lint / typecheck / CI config>
- <lockfiles and dependency manifests>
- <generated or vendored directories>

## Constraints
- `checks.sh` must pass before any keep.
- No new runtime dependencies.
- Behaviour must be identical for existing inputs; no feature removal.
- Simpler code at equal performance counts as a win.

## What's Been Tried
<Empty at start. The loop appends wins, failures and why. Keep it current.>
