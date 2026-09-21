# Autoresearch Toolkit for generic agent hosts

The generic export uses the Agent Plugins manifest format and the same two
skills as the named integrations. It is a portable format, not a sixth tested
application. Use the [shared workflow](../../HARNESS_WORKFLOW.md) for requirements,
engine setup, complete prompts, budgets, evidence and recovery.

## Obtain the package

Extract a generic archive, or export from a full source checkout:

```sh
python3 scripts/export.py --agent generic --output dist/generic/autoresearch-toolkit
```

The root `plugin.json` declares the Agent Plugins 1.0 schema. Keep it alongside
`skills/`, documentation and license notices. A skills package has no CLI binary;
use an engine/generic archive or an independently built engine for Rust mode.
[Agent Plugins specification](https://agent-plugins.org/).

## Register with your host

If the application documents Agent Plugins support, select the entire package
root using that application's local plugin mechanism. Do not assume it uses
another application's command names, namespace or discovery directories. No
universal `plugin install` command exists in this Toolkit.

If the host can read local files and run approved shell commands but has no
plugin loader, use explicit skill paths instead. Start with:

> Read `/absolute/path/to/autoresearch-toolkit/skills/autoresearch-scout/SKILL.md`
> and its referenced workflow. Audit only: explain missing inputs without
> running commands, creating sessions or editing files.

Then use the shared baseline prompt with that same scout path. After accepting
the handoff, ask the host to read
`/absolute/path/to/autoresearch-toolkit/skills/autoresearch-run/SKILL.md` and follow
the shared bounded-run prompt. File-based instruction use does not prove native
plugin support. A host without shell/filesystem access cannot operate this local
Toolkit merely by reading its manifest.

## Acceptance and operating limits

Record the actual host/version, loading method, Toolkit source revision and skill
paths. Confirm it can resolve the references and helper scripts, access the
chosen binary, preserve scope and report the engine verdict unchanged. Start
with audit-only discovery, then explicitly authorize baseline and a bounded run.
The archive tests validate package contents; no unspecified generic host is
claimed as integration-tested.

Use normal host permission controls. The engine remains the owner of Rust
session evidence, budgets and acceptance; the host must not translate every
successful CLI exit into an improvement. Follow the shared stop/status/recovery
procedure after interruption, and review result exports before applying code.

## Upgrade, uninstall and troubleshooting

Stop active work and preserve evidence. Install a new package in a fresh path,
then change the registration or explicit file paths. Keep the old package and
session backup for rollback. To uninstall, remove only the host registration or
Toolkit paths you added; do not remove unrelated host configuration or `.auto/`.

If a manifest is unsupported, use the documented explicit-path workflow and label
it accordingly. If references fail, restore the full package. Missing tools or
denied shell access are host constraints; do not claim an experiment ran when
only instructions were loaded. The shared troubleshooting table covers engine,
baseline and recovery failures.
