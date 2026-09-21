# Autoresearch Toolkit for Codex

Use the two Toolkit skills from Codex CLI or the Codex app. Codex proposes edits;
the optional Rust CLI owns measurement and acceptance. Start with the
[shared workflow](../../HARNESS_WORKFLOW.md) for requirements, engine selection,
baseline/run prompts, evidence, cancellation and recovery.

## Compatibility

The documented path uses project skills rather than a marketplace installation.
On 2026-09-21, Codex **0.154.0** on macOS discovered both project skills as
enabled through its native `skills/list` API in a temporary Git project. No
model turn was started. No minimum version or model-backed Codex session is
claimed as qualified. The native `.codex-plugin`
manifest is included for plugin distribution; this repository is not a registered
marketplace and `codex plugin add` should not be given an invented registry name.

## Install project skills

Extract the Codex archive, or from a source checkout create a folder export:

```sh
python3 scripts/export.py --agent codex --output dist/codex/autoresearch-toolkit
```

Keep that package intact. In the project you want to optimize, copy the two
complete folders into `.agents/skills/`. The following explicit copy refuses
existing destinations, including dangling links:

```sh
python3 - /absolute/path/to/autoresearch-toolkit /absolute/path/to/project <<'PY'
from pathlib import Path
import shutil
import sys
bundle, project = map(Path, sys.argv[1:])
root = project / '.agents/skills'
names = ('autoresearch-scout', 'autoresearch-run')
for name in names:
    if (root / name).exists() or (root / name).is_symlink():
        raise SystemExit(f'Existing installation: {root / name}')
for name in names:
    shutil.copytree(bundle / 'skills' / name, root / name)
PY
codex -C /absolute/path/to/project
```

In the app, open the same project. Codex discovers repository skills under
`.agents/skills`; restart if the new skills do not appear. Use `/skills` or the
skill picker, then select `$autoresearch-scout` and `$autoresearch-run` for the
corresponding shared-workflow prompts. These names apply to the project copies;
plugin-installed skills can have a plugin namespace. Use the name displayed by
your installation. [Official skill documentation](https://learn.chatgpt.com/docs/build-skills).

## Check before optimizing

Confirm both skill names and paths in the picker. Run the shared audit-only
prompt first if a model-backed smoke check is desired. Do not grant optimization
authorization merely to test discovery. Explicitly provide the engine path and
pilot root before baseline preparation. Codex sandbox permissions must cover the
pilot/candidate paths; a Rust contract does not override the host sandbox.

## Update and remove

Stop the Rust session, back up its evidence, and replace only the two project
skill directories with copies from the new package. Keep the previous package
for rollback. To uninstall, remove those two copies and reopen the project; do
not delete the entire `.agents` directory or session evidence. No user-wide
Codex settings need changing for this installation.

If discovery fails, check that the opened project owns the `.agents/skills`
directory and that both folders contain `SKILL.md` plus their resources. If a
plugin and project copies expose the same skills, retain only one method. For
engine/permission errors, follow the shared troubleshooting table.
