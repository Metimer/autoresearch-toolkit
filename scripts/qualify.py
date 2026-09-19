#!/usr/bin/env python3
"""Verify a candidate package and explicitly exercise its bundled demonstrations."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def unpack(archive, destination):
    total = 0
    names = set()
    with tarfile.open(archive, "r:gz") as tar:
        for member in tar:
            path = PurePosixPath(member.name)
            total += member.size
            if (not member.isfile() or path.is_absolute() or ".." in path.parts
                    or not path.parts or path.parts[0] != "autoresearch-toolkit"
                    or member.name in names or len(names) >= 16384 or total > 512 * 1024 * 1024
                    or member.mode not in (0o644, 0o755)):
                raise ValueError("unsafe, duplicate or oversized archive member")
            names.add(member.name)
            target = destination.joinpath(*path.parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            with tar.extractfile(member) as source, target.open("xb") as output:
                shutil.copyfileobj(source, output)
            target.chmod(member.mode)
    return destination / "autoresearch-toolkit"


def verify(root):
    manifest_path = root / "BUNDLE.json"
    if manifest_path.is_symlink():
        raise ValueError("linked package manifest")
    manifest = json.loads(manifest_path.read_text())
    if manifest["schema_version"] != 1 or manifest["profile"] not in ("skills", "engine"):
        raise ValueError("unsupported package manifest")
    actual = set()
    for path in root.rglob("*"):
        if path.is_symlink():
            raise ValueError("linked package resource")
        if path.is_file() and path != manifest_path:
            actual.add(path.relative_to(root).as_posix())
    if actual != set(manifest["files"]):
        raise ValueError("package file inventory differs")
    for name, expected in manifest["files"].items():
        path = root / name
        if (sha(path) != expected["sha256"] or path.stat().st_size != expected["bytes"]
                or bool(path.stat().st_mode & 0o111) != expected["executable"]):
            raise ValueError(f"package integrity mismatch: {name}")
    has_binary = "bin/autoresearch" in actual
    if has_binary != (manifest["profile"] == "engine"):
        raise ValueError("package profile does not match its contents")
    if has_binary:
        dependencies = json.loads((root / "DEPENDENCIES.json").read_text())
        if not dependencies or any(not p["notices"] for p in dependencies):
            raise ValueError("dependency notices are missing")
        for package in dependencies:
            for name in package["notices"]:
                if name not in actual:
                    raise ValueError("dependency notice is not inventoried")
    return manifest


def command(args, cwd):
    output = subprocess.run(args, cwd=cwd, capture_output=True, text=True, timeout=120)
    if output.returncode:
        raise RuntimeError(f"Command failed ({output.returncode}): {args[0]}\n{output.stderr}\n{output.stdout}")
    return output.stdout


def demo(root, name, optimized, temporary):
    source = temporary / f"{name} source é"
    pilot = temporary / f"{name} pilot é"
    shutil.copytree(root / "examples/demos" / name, source)
    pilot.mkdir()
    binary = root / "bin/autoresearch"
    env = {**os.environ, "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
           "GIT_AUTHOR_NAME": "Metimer", "GIT_AUTHOR_EMAIL": "metinamerwane@gmail.com",
           "GIT_COMMITTER_NAME": "Metimer", "GIT_COMMITTER_EMAIL": "metinamerwane@gmail.com"}
    def git(*args):
        return subprocess.check_output(["git", "-C", str(source), *args], env=env, text=True, stderr=subprocess.PIPE).strip()
    git("init", "--template="); git("add", "."); git("commit", "-m", "packaged demonstration")
    original = (source / "src/mode.txt").read_bytes()
    config = json.loads((root / "examples/session.json").read_text())
    config["session_id"] = name
    config["source"] = {"repository": str(source), "commit": git("rev-parse", "HEAD")}
    config["scope"]["protected_paths"] = ["workload.py", "check.py", "bench.py"]
    config["checks"] = [{"executable": sys.executable, "args": ["check.py"], "cwd": "."}]
    config["benchmark"] = {"executable": sys.executable, "args": ["bench.py"], "cwd": "."}
    config["metric"] = {"name": "operations", "unit": "ops", "direction": "lower", "domain": "positive", "minimum_improvement": 1.0}
    config["execution"]["network"] = "allowed"
    config["execution"]["environment"]["set"]["PYTHONDONTWRITEBYTECODE"] = "1"
    config["sampling"]["warmup"] = 0
    config["budget"]["deadline_unix_ms"] = int(time.time() * 1000) + 600000
    path = temporary / f"{name}.json"
    path.write_text(json.dumps(config))
    def cli(*args, session=True):
        flags = ["--session", name] if session else []
        return json.loads(command([str(binary), *args, *flags, "--root", str(pilot), "--json"], temporary))
    cli("init", "--config", str(path), "--operation-id", "init", session=False)
    cli("workspace", "--local-changes", "exclude", "--operation-id", "workspace")
    baseline = cli("baseline", "--operation-id", "baseline")
    if baseline["evaluation"]["report"]["decision"] != "qualified":
        raise ValueError("demo reference did not qualify")
    for candidate, mode, expected in (("improve", optimized, "kept"), ("regress", original.decode(), "discarded")):
        prepared = cli("prepare-candidate", "--candidate", candidate, "--hypothesis", "Reduce repeated work", "--operation-id", "prepare-" + candidate)
        (Path(prepared["artifact"]["path"]) / "src/mode.txt").write_text(mode)
        cli("seal", "--candidate", candidate, "--operation-id", "seal-" + candidate)
        result = cli("evaluate", "--candidate", candidate, "--operation-id", "evaluate-" + candidate)
        if result["evaluation"]["report"]["decision"] != expected:
            raise ValueError(f"demo {name}/{candidate} expected {expected}")
    report = cli("report")
    key = report["evaluation_key"]
    preview = cli("result-preview", "--evaluation", key, "--include-code")
    exported = cli("export-result", "--evaluation", key, "--include-code", "--output", str(temporary / f"{name} result"), "--operation-id", "result")
    if preview["preview"]["sha256"] != exported["result"]["sha256"]:
        raise ValueError("preview and bundle differ")
    if (source / "src/mode.txt").read_bytes() != original or git("status", "--porcelain"):
        raise ValueError("demo source was modified")
    cli("stop", "--operation-id", "stop")
    cli("resume", "--operation-id", "resume")
    return {"project": name, "baseline": "qualified", "improvement": "kept", "regression": "discarded",
            "source_preserved": True, "bundle_verified": True}


def qualify(root):
    root = root.resolve()
    manifest = verify(root)
    with tempfile.TemporaryDirectory(prefix="autoresearch-installed-é-") as directory:
        temporary = Path(directory)
        # A fresh cwd proves that no development-checkout-relative path is needed.
        helper = root / "skills/autoresearch-scout/scripts/measure.py"
        command([sys.executable, str(helper), "--runs", "1", "--warmup", "0", "--", sys.executable, "-c", "pass"], temporary)
        results = []
        if manifest["profile"] == "engine":
            binary = root / "bin/autoresearch"
            doctor = json.loads(command([str(binary), "doctor", "--json"], temporary))
            if doctor["version"] != manifest["version"]:
                raise ValueError("packaged engine version mismatch")
            for name, mode in (("lookup", "indexed"), ("memoization", "cached")):
                results.append(demo(root, name, mode, temporary))
    return {"schema_version": 1, "version": manifest["version"], "profile": manifest["profile"],
            "target": manifest["target"], "package_manifest_sha256": sha(root / "BUNDLE.json"),
            "environment": {"system": platform.platform(), "machine": platform.machine(),
                            "python": platform.python_version()},
            "portable_helper": "passed", "demos": results, "pi_loader": "separate check required" if manifest["pi_adapter"] else "not selected"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--bundle", type=Path)
    source.add_argument("--archive", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        if args.bundle:
            result = qualify(args.bundle)
        else:
            with tempfile.TemporaryDirectory(prefix="autoresearch-extract-") as directory:
                result = qualify(unpack(args.archive, Path(directory)))
        result["archive_sha256"] = sha(args.archive) if args.archive else None
        encoded = json.dumps(result, indent=2) + "\n"
        if args.output:
            with args.output.open("x") as output:
                output.write(encoded)
        print(encoded, end="")
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, tarfile.TarError) as error:
        parser.exit(1, f"Qualification failed: {error}\n")


if __name__ == "__main__":
    main()
