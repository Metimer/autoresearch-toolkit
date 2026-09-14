#!/usr/bin/env python3
"""Export a standalone agent bundle to a NEW folder; never install into an agent."""
from __future__ import annotations

import argparse
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]
MANIFESTS = {
    "codex": ".codex-plugin/plugin.json",
    "claude": ".claude-plugin/plugin.json",
    "cursor": ".cursor-plugin/plugin.json",
    "pi": "package.json",
    "generic": "plugin.json",
}
# Explicit distribution inventory. New resources must be reviewed and added here;
# an unrelated file placed in a skill must never silently enter a release.
PORTABLE_FILES = (
    "README.md",
    "LICENSE",
    "skills/autoresearch-scout/SKILL.md",
    "skills/autoresearch-scout/agents/openai.yaml",
    "skills/autoresearch-scout/assets/prompt.md",
    "skills/autoresearch-scout/scripts/measure.py",
    "skills/autoresearch-run/SKILL.md",
    "skills/autoresearch-run/agents/openai.yaml",
)


def validate_resource(relative: str) -> Path:
    """Reject symlinks in every component below the trusted package root."""
    path = ROOT
    parts = Path(relative).parts
    for index, part in enumerate(parts):
        path = path / part
        if path.is_symlink():
            raise ValueError(f"symlinked resource: {path}")
        valid_type = path.is_file() if index == len(parts) - 1 else path.is_dir()
        if not valid_type:
            raise ValueError(f"missing or invalid resource: {path}")
    return path


def export(agent: str, destination: Path) -> Path:
    manifest = MANIFESTS[agent]
    destination = destination.expanduser().absolute()
    if destination.name != "autoresearch-toolkit":
        raise ValueError("destination folder must be named autoresearch-toolkit")
    if destination.exists() or destination.is_symlink():
        raise FileExistsError(f"refusing to overwrite {destination}")
    if destination.resolve().is_relative_to((ROOT / "skills").resolve()):
        raise ValueError("destination must be outside the source skills directory")
    resources = [validate_resource(name) for name in (*PORTABLE_FILES, manifest)]
    destination.mkdir(parents=True, exist_ok=False)
    # Only maintained portable resources; no originals, sessions or local config.
    # If I/O fails, leave partial output for inspection, never recursively delete.
    for source in resources:
        target = destination / source.relative_to(ROOT)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    return destination


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--agent", required=True, choices=MANIFESTS)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        result = export(args.agent, args.output)
    except (OSError, ValueError) as exc:
        parser.exit(1, f"Export failed: {exc}\n")
    print(f"Exported {args.agent}: {result}\nNo agent configuration changed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
