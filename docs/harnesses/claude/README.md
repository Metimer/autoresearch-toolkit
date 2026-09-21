# Autoresearch Toolkit for Claude Code

Load Toolkit as a local Claude Code plugin. Its skills use either the Rust CLI
or the portable measurement workflow. The [shared workflow](../../HARNESS_WORKFLOW.md)
covers requirements, engine selection, complete prompts, results and recovery.

## Compatibility and installation

Local manifest validation uses Claude Code **2.1.278** on macOS. This checks
structure; an interactive model-backed run is a separate check. No older minimum
version is claimed. Use a Claude archive or create a folder export from source:

```sh
python3 scripts/export.py --agent claude --output dist/claude/autoresearch-toolkit
```

Keep the package intact. Validate and start Claude from the project to optimize:

```sh
claude plugin validate /absolute/path/to/autoresearch-toolkit
cd /absolute/path/to/project
claude --plugin-dir /absolute/path/to/autoresearch-toolkit
```

The directory must contain `.claude-plugin/plugin.json` and `skills/` at its root.
`--plugin-dir` loads a local plugin for that invocation without registering a
marketplace. [Official local plugin instructions](https://code.claude.com/docs/en/plugins).

## Discover and invoke

In Claude's slash-command picker, check for:

```text
/autoresearch-toolkit:autoresearch-scout
/autoresearch-toolkit:autoresearch-run
```

Select scout with the shared audit-only prompt to check loading, then use it for
baseline preparation when authorized. Select run with the bounded optimization
prompt only after the baseline handoff. The plugin namespace distinguishes these
from standalone skills. [Official plugin reference](https://code.claude.com/docs/en/plugins-reference).

Pass the exact `autoresearch` binary path and pilot root in your request. Allow
access only to the paths and commands needed by that session through Claude's
normal permission controls. The plugin does not bypass permissions or install a
binary. `--plugin-dir` alone never initializes a Rust session.

## Stop, update and uninstall

Interrupt Claude to stop the current agent turn, and use the shared engine
`stop`/`status` commands to verify that a Rust evaluation has settled. A new
Claude conversation does not reset engine budgets or automatically resume work.

For an update, stop active work, keep the previous package, and relaunch with the
new package's absolute path. Restarting without `--plugin-dir` removes this
session-local registration. If you separately installed a marketplace version,
manage that installation through Claude's plugin controls as well. Keep session
evidence and the target project when removing the package.

## Troubleshooting and acceptance

A missing command usually means the wrong plugin root, an outdated session, or
a second installation with the same name. Inspect the plugin listing and relaunch
with one registration. A manifest validation success alone does not prove the
skills appeared in that listing. Missing references mean a partial skill copy;
restore the full export. Engine errors use the shared troubleshooting procedure.

For interactive acceptance, record the Claude version, Toolkit source revision,
visible names, audit-only outcome, and whether an explicitly authorized baseline
and bounded run preserved the source. Such a model-backed acceptance session is
not included in the automated package qualification.
