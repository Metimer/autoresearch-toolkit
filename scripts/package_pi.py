#!/usr/bin/env python3
"""Build and qualify Ultramarine's npm tarball; never log in, tag or publish."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

import export as exporter
import qualify
import release

ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = "packaging/pi/package.json"
README = "packaging/pi/README.md"
FILES = tuple(dict.fromkeys((
    *exporter.PORTABLE_FILES,
    exporter.HARNESS_GUIDES["pi"],
    "adapters/pi/index.ts", "adapters/pi/transport.ts",
)))


def build(output: Path, require_clean: bool = False) -> Path:
    output = output.expanduser().absolute()
    if output.exists() or output.is_symlink():
        raise ValueError("output directory must be new")
    if output.resolve().is_relative_to(ROOT) and not output.resolve().is_relative_to(ROOT / "dist"):
        raise ValueError("use dist/ or a directory outside the checkout")
    revision = release.run(["git", "rev-parse", "HEAD"]).strip()
    dirty = bool(release.run(["git", "status", "--porcelain"]).strip())
    if require_clean and dirty:
        raise ValueError("commit the final changes before building a release candidate")
    for name in (*FILES, TEMPLATE, README):
        exporter.validate_resource(name)
    package = json.loads((ROOT / TEMPLATE).read_text())
    adapter = json.loads((ROOT / "adapters/pi/package.json").read_text())
    version = json.loads((ROOT / "package.json").read_text())["version"]
    if package["version"] != version or adapter["version"] != version:
        raise ValueError("npm, adapter and engine package versions must match")
    package.pop("private", None)
    package["files"] = sorted({*FILES, "package.json", "BUNDLE.json"})
    output.mkdir(parents=True, exist_ok=False)
    staging = output / "package"
    staging.mkdir()
    for name in FILES:
        destination = staging / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / name, destination)
    shutil.copy2(ROOT / README, staging / "README.md")
    (staging / "package.json").write_text(json.dumps(package, indent=2) + "\n")
    manifest = {
        "schema_version": 1, "version": version, "profile": "pi-npm", "agent": "pi",
        "target": None, "pi_adapter": True, "npm_name": package["name"],
        "source_commit": revision, "source_dirty": dirty,
        "tested_pi": adapter["devDependencies"]["@earendil-works/pi-coding-agent"],
        "files": release.inventory(staging),
    }
    (staging / "BUNDLE.json").write_text(json.dumps(manifest, indent=2) + "\n")
    with tempfile.TemporaryDirectory(prefix="ultramarine-npm-cache-") as cache:
        subprocess.run([
            "npm", "pack", "--offline", "--ignore-scripts", "--json",
            "--cache", cache, "--pack-destination", str(output),
        ], cwd=staging, check=True, stdout=subprocess.PIPE, text=True)
    archives = list(output.glob("*.tgz"))
    if len(archives) != 1:
        raise ValueError("npm did not produce exactly one tarball")
    archive = archives[0]
    with tempfile.TemporaryDirectory(prefix="ultramarine-packed-") as directory:
        extracted = qualify.unpack(archive, Path(directory), root_name="package")
        verify(extracted, require_clean)
    (output / "SHA256SUMS").write_text(f"{qualify.sha(archive)}  {archive.name}\n")
    return archive


def verify(root: Path, require_clean: bool = False) -> dict:
    manifest = qualify.verify(root)
    package = json.loads((root / "package.json").read_text())
    if manifest["profile"] != "pi-npm" or package.get("private"):
        raise ValueError("not a public Pi npm package")
    if require_clean and manifest["source_dirty"]:
        raise ValueError("package was built from uncommitted changes")
    if package["name"] != manifest["npm_name"] or package["version"] != manifest["version"]:
        raise ValueError("npm manifest does not match bundle identity")
    if "pi-package" not in package.get("keywords", []):
        raise ValueError("missing Pi catalog keyword")
    if package.get("dependencies") or package.get("scripts") or package.get("devDependencies"):
        raise ValueError("the npm distribution must use host peers and have no lifecycle scripts")
    if package.get("peerDependencies") != {"@earendil-works/pi-coding-agent": "*", "typebox": "*"}:
        raise ValueError("declare Pi-provided modules as host peers")
    resources = package.get("pi", {})
    if {key: resources.get(key) for key in ("extensions", "skills")} != {
        "extensions": ["./adapters/pi/index.ts"], "skills": ["./skills"],
    }:
        raise ValueError("unexpected Pi resources")
    # Check the extracted artifact, including every relative documentation link.
    for page in root.rglob("*.md"):
        for link in re.findall(r"\[[^\]]+\]\(([^)]+)\)", page.read_text()):
            if "://" in link or link.startswith("#"):
                continue
            target = (page.parent / link.split("#", 1)[0]).resolve()
            if not target.is_relative_to(root.resolve()) or not target.is_file():
                raise ValueError(f"broken packaged link: {page.relative_to(root)} -> {link}")
    return manifest


def check(archive: Path, engine: Path, pi: Path, require_clean: bool = False) -> dict:
    archive, engine, pi = archive.resolve(), engine.resolve(), pi.resolve()
    with tempfile.TemporaryDirectory(prefix="ultramarine-installed-é-") as directory:
        temporary = Path(directory)
        extracted = qualify.unpack(archive, temporary, root_name="package")
        manifest = verify(extracted, require_clean)
        # Pi's npm installer suppresses host peer installation. Reproduce that
        # install in a fresh prefix, offline, then test the installed files.
        consumer = temporary / "consumer"
        consumer.mkdir()
        subprocess.run([
            "npm", "install", str(archive), "--prefix", str(consumer),
            "--offline", "--legacy-peer-deps", "--ignore-scripts", "--no-audit", "--no-fund",
            "--cache", str(temporary / "npm-cache"),
        ], check=True, timeout=60, stdout=sys.stderr)
        root = consumer / "node_modules" / manifest["npm_name"]
        verify(root, require_clean)
        result = qualify.qualify(root)
        loaded = json.loads(qualify.command([
            "node", str(ROOT / "scripts/qualify_npm.mjs"), str(root), str(pi), str(engine),
        ], temporary))
        # Exercise the existing engine/cancellation suite against the installed
        # extension, using the independent development Pi host.
        env = {**os.environ, "ULTRAMARINE_TEST_EXTENSION": str(root / "adapters/pi/index.ts"),
               "ULTRAMARINE_TEST_ENGINE": str(engine)}
        subprocess.run(["node", "--test", str(ROOT / "adapters/pi/tests/adapter.test.mjs")],
                       cwd=temporary, env=env, check=True, timeout=180, stdout=sys.stderr)
        result.update(loaded)
        result.update({"archive_sha256": qualify.sha(archive), "npm_name": manifest["npm_name"],
                       "source_commit": manifest["source_commit"], "source_dirty": manifest["source_dirty"],
                       "npm_install": "passed", "packaged_workflow_and_cancellation": "passed"})
        return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    builder = commands.add_parser("build", help="stage explicit resources and run npm pack offline")
    builder.add_argument("--output", type=Path, required=True)
    checker = commands.add_parser("check", help="test the actual tarball outside the checkout")
    checker.add_argument("--archive", type=Path, required=True)
    checker.add_argument("--engine", type=Path, default=ROOT / "target/debug/autoresearch")
    checker.add_argument("--pi", type=Path, default=ROOT / "adapters/pi/node_modules/@earendil-works/pi-coding-agent")
    checker.add_argument("--output", type=Path)
    for command in (builder, checker):
        command.add_argument("--require-clean", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "build":
            print(build(args.output, args.require_clean))
        else:
            result = check(args.archive, args.engine, args.pi, args.require_clean)
            encoded = json.dumps(result, indent=2) + "\n"
            if args.output:
                with args.output.open("x") as output:
                    output.write(encoded)
            print(encoded, end="")
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, tarfile.TarError) as error:
        parser.exit(1, f"Pi npm {args.command} failed: {error}\n")


if __name__ == "__main__":
    main()
