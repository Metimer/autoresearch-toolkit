#!/usr/bin/env python3
"""Collect qualified CI artifacts for a GitHub/npm release; never publish."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import shutil
import tarfile
import tempfile

import package_pi
import qualify
import release


def inspect(directory: Path, commit: str, profile: str, agent: str, target: str | None) -> Path:
    npm = profile == "pi-npm"
    archives = list(directory.glob("*.tgz" if npm else "*.tar.gz"))
    if len(archives) != 1:
        raise ValueError(f"expected one archive in {directory}")
    archive = archives[0]
    sha = qualify.sha(archive)
    if (directory / "SHA256SUMS").read_text().strip() != f"{sha}  {archive.name}":
        raise ValueError(f"checksum mismatch: {directory.name}")
    report = json.loads((directory / "qualification.json").read_text())
    with tempfile.TemporaryDirectory(prefix="ultramarine-publication-") as temporary:
        root = qualify.unpack(archive, Path(temporary), root_name="package" if npm else "autoresearch-toolkit")
        manifest = package_pi.verify(root, require_clean=True) if npm else qualify.verify(root)
        if manifest["source_dirty"] or manifest["source_commit"] != commit:
            raise ValueError(f"dirty or different source revision: {directory.name}")
        expected = {"version": manifest["version"], "profile": profile, "target": target}
        if manifest["agent"] != agent or any(manifest[key] != value for key, value in expected.items()):
            raise ValueError(f"unexpected archive profile: {directory.name}")
        if any(report.get(key) != value for key, value in expected.items()):
            raise ValueError(f"report profile mismatch: {directory.name}")
        manifest_sha = qualify.sha(root / "BUNDLE.json")
        if (report.get("archive_sha256") != sha or report.get("package_manifest_sha256") != manifest_sha
                or report.get("portable_helper") != "passed"):
            raise ValueError(f"qualification does not match archive: {directory.name}")
        if profile == "engine":
            demos = report.get("demos", [])
            if (len(demos) != 2 or {demo.get("project") for demo in demos} != {"lookup", "memoization"}
                    or any(demo.get("baseline") != "qualified" or demo.get("improvement") != "kept"
                           or demo.get("regression") != "discarded" or demo.get("source_preserved") is not True
                           or demo.get("bundle_verified") is not True for demo in demos)):
                raise ValueError(f"engine demos not qualified: {directory.name}")
            pi = json.loads((directory / "pi-qualification.json").read_text())
            if (pi.get("package_manifest_sha256") != manifest_sha or pi.get("packaged_loader") != "passed"
                    or pi.get("engine_negotiation") != "passed" or pi.get("provider_requests") != 0):
                raise ValueError(f"native Pi check not qualified: {directory.name}")
        if npm:
            for key in ("npm_install", "pi_loader", "skills_loader", "engine_negotiation", "packaged_workflow_and_cancellation"):
                if report.get(key) != "passed":
                    raise ValueError(f"npm check {key} not qualified: {directory.name}")
            if report.get("source_commit") != commit or report.get("source_dirty") is not False:
                raise ValueError(f"npm source report mismatch: {directory.name}")
    return archive


def collect(artifacts: Path, output: Path, commit: str) -> Path:
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("provide the full 40-character qualified commit SHA")
    if output.exists() or output.is_symlink():
        raise ValueError("output directory must be new")
    selected = []
    entries = [(f"candidate-{target}", "engine", "pi", target) for target in release.TARGETS]
    entries += [(f"candidate-skills-{agent}", "skills", agent, None) for agent in release.exporter.MANIFESTS]
    entries += [(f"candidate-npm-{host}", "pi-npm", "pi", None) for host in ("ubuntu-24.04", "macos-15")]
    for name, profile, agent, target in entries:
        archive = inspect(artifacts / name, commit, profile, agent, target)
        selected.append((name, archive))
    # Both hosts must pass; publish the npm tarball tested on Linux. Keep the
    # second tarball and report under a distinct filename for traceability.
    output.mkdir(parents=True, exist_ok=False)
    for name, archive in selected:
        filename = archive.name if name != "candidate-npm-macos-15" else f"macos-verified-{archive.name}"
        shutil.copy2(archive, output / filename)
        for report in archive.parent.glob("*qualification.json"):
            shutil.copy2(report, output / f"{name}-{report.name}")
    sums = "".join(f"{qualify.sha(path)}  {path.name}\n" for path in sorted(output.iterdir()))
    (output / "SHA256SUMS").write_text(sums)
    return output


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()
    try:
        print(collect(args.artifacts, args.output, args.commit))
    except (OSError, ValueError, KeyError, tarfile.TarError) as error:
        parser.exit(1, f"Publication preparation failed: {error}\n")


if __name__ == "__main__":
    main()
