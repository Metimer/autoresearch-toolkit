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
HARNESS_GUIDES = {agent: f"docs/harnesses/{agent}/README.md" for agent in MANIFESTS}
# Explicit distribution inventory. New resources must be reviewed and added here;
# an unrelated file placed in a skill must never silently enter a release.
# Both skill modes are shipped; the separately installed Rust binary is excluded.
PORTABLE_FILES = (
    "README.md",
    "LICENSE",
    "THIRD_PARTY_NOTICES.md",
    "docs/HARNESS_WORKFLOW.md",
    "docs/INSTALL.md",
    "docs/RELEASE_NOTES.md",
    "docs/RUST_ENGINE.md",
    "docs/RESULTS.md",
    "schemas/session-v2.schema.json",
    "adapters/pi/README.md",
    "skills/autoresearch-scout/SKILL.md",
    "skills/autoresearch-scout/agents/openai.yaml",
    "skills/autoresearch-scout/assets/prompt.md",
    "skills/autoresearch-scout/assets/session.json",
    "skills/autoresearch-scout/references/engine.md",
    "skills/autoresearch-scout/references/portable.md",
    "skills/autoresearch-scout/scripts/measure.py",
    "skills/autoresearch-run/SKILL.md",
    "skills/autoresearch-run/references/engine.md",
    "skills/autoresearch-run/references/portable.md",
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
    resources = [validate_resource(name) for name in (*PORTABLE_FILES, manifest, HARNESS_GUIDES[agent])]
    destination.mkdir(parents=True, exist_ok=False)
    # Only maintained portable resources; no originals, sessions or local config.
    # If I/O fails, leave partial output for inspection, never recursively delete.
    for source in resources:
        target = destination / source.relative_to(ROOT)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    (destination / "README.md").write_text(
        f"# Autoresearch Toolkit for {agent}\n\n"
        "Created and maintained by **Metimer**.\n\n"
        f"Start with the [complete {agent} guide]({HARNESS_GUIDES[agent]}), then follow the "
        "[shared workflow](docs/HARNESS_WORKFLOW.md) for engine setup, baseline preparation, "
        "bounded optimization, stopping and recovery.\n\n"
        "This skills-only folder contains both complete skills, their resources, the selected "
        "host manifest and documentation. It contains no Rust binary, extension code or build tooling. "
        "The Pi adapter reference is documentation only. Rust mode needs an independently installed "
        "engine; portable measurements need Git and Python 3.10+.\n\n"
        "See [installation and compatibility](docs/INSTALL.md) and [candidate release notes](docs/RELEASE_NOTES.md). "
        "Exporting does not change agent settings or install dependencies. Keep the "
        "[MIT license](LICENSE) and [third-party notices](THIRD_PARTY_NOTICES.md).\n"
    )
    return destination


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--agent", required=True, choices=MANIFESTS)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--profile", choices=("skills", "engine"),
                        help="build a versioned release archive in a new output directory")
    parser.add_argument("--target")
    parser.add_argument("--pi-adapter", action="store_true")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    if args.profile:
        from release import build
        import subprocess
        try:
            print(build(args.profile, args.agent, args.output, args.target, args.pi_adapter, args.offline))
            return 0
        except (OSError, ValueError, subprocess.CalledProcessError) as exc:
            parser.exit(1, f"Release export failed: {exc}\n")
    if args.target or args.pi_adapter or args.offline:
        parser.error("release options require --profile")
    try:
        result = export(args.agent, args.output)
    except (OSError, ValueError) as exc:
        parser.exit(1, f"Export failed: {exc}\n")
    print(f"Exported {args.agent}: {result}\nNo agent configuration changed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
