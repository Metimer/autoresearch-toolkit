#!/usr/bin/env python3
"""Build a local candidate archive from an explicit inventory; never publish it."""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

import export as exporter

ROOT = Path(__file__).resolve().parents[1]
TARGETS = ("aarch64-apple-darwin", "x86_64-apple-darwin",
           "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu")
ENGINE_FILES = (
    "examples/session.json", "scripts/qualify.py",
    "examples/demos/README.md",
    *(f"examples/demos/{demo}/{name}" for demo in ("lookup", "memoization")
      for name in ("workload.py", "check.py", "bench.py", "src/mode.txt")),
)
PI_FILES = ("index.ts", "transport.ts", "package.json", "package-lock.json", "README.md")


def run(args, cwd=ROOT):
    return subprocess.check_output(args, cwd=cwd, text=True)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest() if hasattr(hashlib, "file_digest") else hashlib.sha256(stream.read()).hexdigest()


def copy(relative, destination):
    source = exporter.validate_resource(relative)
    target = destination / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)


def licenses(metadata, destination):
    """Include the resolved normal/build graph, with original license notices."""
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    pending = [p["id"] for p in packages.values() if p["name"] == "autoresearch-cli"]
    seen = set()
    while pending:
        key = pending.pop()
        if key in seen:
            continue
        seen.add(key)
        pending.extend(d["pkg"] for d in nodes[key]["deps"]
                       if any(k["kind"] != "dev" for k in d["dep_kinds"]))
    inventory = []
    for package in sorted((packages[key] for key in seen), key=lambda p: p["name"]):
        if package["source"] is None:
            continue
        root = Path(package["manifest_path"]).parent
        texts = sorted(path for path in root.rglob("*") if path.is_file() and
                       (path.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT"))
                        or (package.get("license_file") and path == root / package["license_file"])))
        if not texts or not (package.get("license") or package.get("license_file")):
            raise ValueError(f"missing license evidence for {package['name']}")
        names = []
        for source in texts:
            if source.is_symlink() or not source.resolve().is_relative_to(root.resolve()):
                raise ValueError("unsafe dependency license path")
            target = destination / f"licenses/{package['name']}-{package['version']}" / source.relative_to(root)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            names.append(target.relative_to(destination).as_posix())
        inventory.append({"name": package["name"], "version": package["version"],
                          "license": package.get("license"), "authors": package["authors"],
                          "repository": package.get("repository"), "notices": names})
    sysroot = Path(run(["rustc", "--print", "sysroot"]).strip())
    rust_notices = []
    for name in ("LICENSE-MIT", "LICENSE-APACHE", "COPYRIGHT"):
        source = sysroot / "share/doc/rust" / name
        if not source.is_file():
            raise ValueError(f"Rust distribution notice missing: {name}")
        target = destination / "licenses/rust" / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        rust_notices.append(target.relative_to(destination).as_posix())
    inventory.append({"name": "Rust distribution", "version": run(["rustc", "--version"]).strip(),
                      "license": "MIT OR Apache-2.0; third-party notices in COPYRIGHT", "notices": rust_notices})
    (destination / "DEPENDENCIES.json").write_text(json.dumps(inventory, indent=2) + "\n")


def inventory(root):
    files = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValueError("package contains a symlink")
        if path.is_file():
            files[path.relative_to(root).as_posix()] = {
                "sha256": digest(path), "bytes": path.stat().st_size,
                "executable": bool(path.stat().st_mode & 0o111),
            }
    return files


def archive(root, path):
    # Stable archive metadata; binary bytes remain toolchain/platform dependent.
    with path.open("xb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w") as tar:
            for source in sorted(root.rglob("*")):
                if not source.is_file():
                    continue
                info = tarfile.TarInfo("autoresearch-toolkit/" + source.relative_to(root).as_posix())
                info.size = source.stat().st_size
                info.mode = 0o755 if source.stat().st_mode & 0o111 else 0o644
                with source.open("rb") as content:
                    tar.addfile(info, content)


def build(profile, agent, output, target=None, pi=False, offline=False):
    output = output.absolute()
    if output.exists() or output.is_symlink():
        raise ValueError("release output directory must be new")
    resolved = output.resolve()
    if resolved.is_relative_to(ROOT) and not resolved.is_relative_to(ROOT / "dist"):
        raise ValueError("use dist/ or an output directory outside the source checkout")
    if pi and (profile != "engine" or agent != "pi"):
        raise ValueError("the Pi adapter requires --profile engine --agent pi")
    if target and profile != "engine":
        raise ValueError("a target is only valid for the engine profile")
    version = json.loads((ROOT / "package.json").read_text())["version"]
    metadata = None
    if profile == "engine":
        host = next(line.split(": ", 1)[1] for line in run(["rustc", "-vV"]).splitlines() if line.startswith("host: "))
        target = target or host
        if target not in TARGETS or target != host:
            raise ValueError("build and qualify on the native target; cross-built binaries are not accepted")
        flags = ["--locked", "--target", target] + (["--offline"] if offline else [])
        subprocess.run(["cargo", "build", "--release", "-p", "autoresearch-cli", *flags], cwd=ROOT, check=True)
        metadata = json.loads(run(["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target, *(["--offline"] if offline else [])]))
    resources = (*(ENGINE_FILES if metadata else ()),
                 *(f"adapters/pi/{name}" for name in PI_FILES if pi),
                 *(("scripts/qualify_pi.mjs",) if pi else ()))
    for name in resources:
        exporter.validate_resource(name)
    with tempfile.TemporaryDirectory(prefix="autoresearch-package-") as temporary:
        root = exporter.export(agent, Path(temporary) / "autoresearch-toolkit")
        for name in resources:
            copy(name, root)
        if metadata:
            binary = Path(metadata["target_directory"]) / target / "release/autoresearch"
            found = json.loads(run([str(binary), "--version", "--json"]))
            if found["version"] != version:
                raise ValueError("engine and package versions differ")
            (root / "bin").mkdir()
            shutil.copy2(binary, root / "bin/autoresearch")
            licenses(metadata, root)
        if pi:
            adapter_path = root / "adapters/pi/package.json"
            adapter = json.loads(adapter_path.read_text())
            adapter["scripts"] = {"test": "node ../../scripts/qualify_pi.mjs ../.."}
            adapter_path.write_text(json.dumps(adapter, indent=2) + "\n")
            manifest_path = root / "package.json"
            manifest = json.loads(manifest_path.read_text())
            manifest["pi"]["extensions"] = ["./adapters/pi/index.ts"]
            manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
        (root / "README.md").write_text(
            f"# Autoresearch Toolkit {version}\n\n"
            "Created and maintained by **Metimer**.\n\n"
            f"Profile: **{profile}**. Agent: **{agent}**. "
            f"Target: **{target or 'portable'}**. Pi engine adapter: **{'included' if pi else 'not included'}**.\n\n"
            f"Start with the [complete {agent} guide]({exporter.HARNESS_GUIDES[agent]}) and "
            "[shared agent workflow](docs/HARNESS_WORKFLOW.md).\n\n"
            "See [installation and compatibility](docs/INSTALL.md) and [candidate release notes](docs/RELEASE_NOTES.md). "
            "This archive is a build for release qualification. Its version alone does not establish "
            "qualification or publication; check the source revision and its CI reports.\n\n"
            "The skills are in `skills/`. Engine profiles also include `bin/autoresearch`, the engine guides, "
            "dependency notices and two reproducible demonstrations. Skills-only packages require an independently "
            "installed engine for Rust mode; their portable mode needs Git and Python 3.10+. "
            "Importing this package does not download dependencies or launch experiments.\n")
        revision = run(["git", "rev-parse", "HEAD"]).strip()
        dirty = bool(run(["git", "status", "--porcelain"]).strip())
        manifest = {"schema_version": 1, "version": version, "profile": profile, "agent": agent,
                    "target": target, "pi_adapter": pi, "source_commit": revision,
                    "source_dirty": dirty, "files": inventory(root)}
        (root / "BUNDLE.json").write_text(json.dumps(manifest, indent=2) + "\n")
        # Reserve output only after every resource, binary and license is ready.
        output.mkdir(parents=True, exist_ok=False)
        name = f"autoresearch-toolkit-{version}-{profile}-{agent}-{target or 'portable'}{'-with-pi' if pi else ''}.tar.gz"
        path = output / name
        archive(root, path)
        (output / "SHA256SUMS").write_text(f"{digest(path)}  {name}\n")
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", required=True, choices=("skills", "engine"))
    parser.add_argument("--agent", required=True, choices=exporter.MANIFESTS)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--pi-adapter", action="store_true")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    try:
        print(build(args.profile, args.agent, args.output, args.target, args.pi_adapter, args.offline))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Release build failed: {error}\n")


if __name__ == "__main__":
    main()
