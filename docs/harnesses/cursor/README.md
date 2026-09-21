# Autoresearch Toolkit for Cursor

Toolkit supplies two agent skills and a Cursor plugin manifest. Use the
[shared workflow](../../HARNESS_WORKFLOW.md) for prerequisites, engine setup,
complete baseline/run prompts, results, stop/resume and troubleshooting.

## Compatibility

Documentation was checked on 2026-09-21; the local editor reports **3.19.13**.
The editor's interactive skill picker and a model-backed run have not been
qualified here. Installation policy can differ by organization. Native engine
and archive tests do not establish that Cursor loaded this plugin.

## Install project skills

Extract the Cursor archive, or export from a full source checkout:

```sh
python3 scripts/export.py --agent cursor --output dist/cursor/autoresearch-toolkit
```

Keep the package and notices intact. Copy both complete skill folders into the
target project's `.cursor/skills/`, stopping if either destination exists:

```sh
python3 - /absolute/path/to/autoresearch-toolkit /absolute/path/to/project <<'PY'
from pathlib import Path
import shutil
import sys
bundle, project = map(Path, sys.argv[1:])
root = project / '.cursor/skills'
names = ('autoresearch-scout', 'autoresearch-run')
for name in names:
    if (root / name).exists() or (root / name).is_symlink():
        raise SystemExit(f'Existing installation: {root / name}')
for name in names:
    shutil.copytree(bundle / 'skills' / name, root / name)
PY
```

Open that project in Cursor. Check **Customize → Skills**, then type `/` in Agent
chat and select `autoresearch-scout` or `autoresearch-run`. Use the shared
audit-only prompt before authorizing preparation or optimization.
[Official skills guide](https://cursor.com/docs/skills).

## Alternative: local plugin

Instead of project skill copies, copy the complete Cursor export into a new
`~/.cursor/plugins/local/autoresearch-toolkit` directory. Preserve the hidden
`.cursor-plugin` directory. Reload the window and inspect Customize. Local
plugin imports must be permitted by organization policy; an installed marketplace
copy takes precedence over a local plugin of the same name. Do not symlink to a
repository outside the local plugin directory.
[Official local plugin guide](https://cursor.com/docs/plugins#test-plugins-locally).

Choose one installation method. A marketplace listing is not created by placing
this repository on GitHub. This guide targets local Agent execution, not Cloud
Agents: remote machines would need their own binary, files and qualification.

## Engine, updates and removal

Provide the full binary path and pilot root in the shared scout prompt. Cursor
launched from a desktop may not have your terminal's PATH. Use its normal command
approval controls; the Rust engine cannot grant permissions denied by Cursor.

Stop the session and check engine status before updating. Back up the old package
and sessions, then replace only the two skill copies or the local plugin folder.
Reload the window and check for duplicates. To uninstall, remove only the
Toolkit folders you installed; leave other skills, `.auto/` evidence and source
files in place. Remove a separately installed marketplace copy through Customize.

If skills are absent, inspect project selection, directory layout and policy.
Record the Cursor version, source revision and discovered skill paths for a
manual acceptance check. Merely opening the project or seeing the manifest is
not proof that a skill has loaded. Follow the shared workflow for the separate
model-backed baseline/run check.
