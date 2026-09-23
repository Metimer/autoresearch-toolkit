# Autoresearch Toolkit for Pi

Pi can load Toolkit's skills alone, or add the optional Rust engine extension.
Use the [shared workflow](../../HARNESS_WORKFLOW.md) for prerequisites, scope,
budgets, complete prompts, result interpretation and recovery.

## Compatibility and profiles

The extension is pinned to **Pi 0.85.1**, **Node.js 24**, and Toolkit
**1.0.0**. Its real loader, engine negotiation, workflow and cancellation
have integration coverage; packaged loading was qualified on four native targets.
The skills-only profile registers no engine extension. Neither profile is an
npm registry release; root packages remain private.
On 2026-09-21, Pi's native skill loader also discovered both skills from a fresh
folder export with no diagnostics or provider request.

Create a folder export from source, or extract a skills/Pi archive:

```sh
python3 scripts/export.py --agent pi --output dist/pi/autoresearch-toolkit
```

## Load skills without the extension

With Pi 0.85.1 installed separately, start from the target project:

```sh
cd /absolute/path/to/project
pi --no-skills --skill /absolute/path/to/autoresearch-toolkit/skills
```

Explicit skill paths still load with `--no-skills`. This selects the Toolkit
skills without writing a package registration to Pi settings. Other configured
Pi extensions are unaffected. Check the startup resource list and invoke:

```text
/skill:autoresearch-scout
/skill:autoresearch-run
```

Use the audit-only, baseline and bounded-run prompts from the shared workflow.
Rust mode still works through shell tools if you provide a compatible binary.
The two commands are skills, not an automatic experiment loop.
[Pi skill documentation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md).

## Add the native engine tool

Use an engine archive built with `--agent pi --pi-adapter`. Qualify its inventory
before installing dependencies, as described in [installation](../../INSTALL.md).
Then install the adapter's pinned dependencies explicitly:

```sh
npm --prefix /absolute/path/to/autoresearch-toolkit/adapters/pi ci \
  --ignore-scripts --no-audit --no-fund
```

Prepare a real session contract with the scout and initialize the session with
the Rust CLI. The extension attaches to an existing session; it cannot infer its
source, scope, budget or deadline. Start the packaged Pi executable from your
project, selecting both skills and the extension explicitly:

```sh
/absolute/path/to/autoresearch-toolkit/adapters/pi/node_modules/.bin/pi \
  --no-skills --skill /absolute/path/to/autoresearch-toolkit/skills \
  -e /absolute/path/to/autoresearch-toolkit/adapters/pi/index.ts \
  --autoresearch-engine /absolute/path/to/autoresearch-toolkit/bin/autoresearch \
  --autoresearch-root /absolute/path/to/pilot --autoresearch-session SESSION_ID
```

The tool is `autoresearch_engine`. Its mutating actions require explicit
operation IDs. Read the [adapter reference](../../../adapters/pi/README.md) for
all actions, cancellation guarantees and limitations. That reference is included
in skills exports too, but the extension code is only present when selected.
A full source checkout uses its built CLI path instead of `bin/autoresearch`.

## Stop, update and remove

Pi cancellation and `/autoresearch-stop` cancel the currently owned engine call.
Use the tool's `stop` action to mark an idle session stopped. Session transitions
cancel owned work; they never automatically resume an experiment. Check Rust
status before reopening a session. Budgets and deadlines survive restart.

Keep the previous package and a session backup before selecting new skill,
extension and binary paths. Uninstall this invocation-based setup by dropping
`--skill`, `-e` and the Toolkit flags from the next launch. No persistent package
registration was created. If you separately used `pi install`, remove that
registration with Pi's package manager at the same scope; preserve session data.

## Troubleshooting and acceptance

If skill commands are absent, inspect loaded skill paths and Pi's
`enableSkillCommands` setting. Duplicate tools/skills indicate mixed explicit
paths and package registrations. Missing session or incompatible engine errors
require the correct pilot/session and exact pinned binary, not a new empty
session. Keep the full skill folders so referenced resources remain reachable.

The packaged loader check needs no provider request:

```sh
node /absolute/path/to/autoresearch-toolkit/scripts/qualify_pi.mjs \
  /absolute/path/to/autoresearch-toolkit
```

It is available in engine/Pi archives after dependencies are installed. A real
model-backed optimization remains a separate, explicitly authorized user action.
