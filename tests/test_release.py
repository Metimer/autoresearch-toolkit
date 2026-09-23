import io
import json
from pathlib import Path
import sys
import re
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import release
import qualify


class ReleaseTests(unittest.TestCase):
    def test_all_skills_archives_work_outside_checkout_and_describe_their_contents(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            for agent in release.exporter.MANIFESTS:
                with self.subTest(agent=agent):
                    archive = release.build("skills", agent, base / agent)
                    root = qualify.unpack(archive, base / (agent + "-extracted"))
                    result = qualify.qualify(root)
                    self.assertEqual(result["profile"], "skills")
                    self.assertEqual(result["portable_helper"], "passed")
                    manifest = qualify.verify(root)
                    self.assertFalse(manifest["pi_adapter"])
                    self.assertNotIn("bin/autoresearch", manifest["files"])
                    self.assertFalse(any(name.startswith(("originals/", ".auto/", "node_modules/")) for name in manifest["files"]))
                    self.assertEqual(manifest["version"], "1.0.0")
                    self.assertIn(release.exporter.HARNESS_GUIDES[agent], manifest["files"])
                    self.assertIn(release.exporter.HARNESS_GUIDES[agent], (root / "README.md").read_text())
                    for page in root.rglob("*.md"):
                        for link in re.findall(r"\[[^\]]+\]\(([^)]+)\)", page.read_text()):
                            if "://" in link or link.startswith("#"):
                                continue
                            resource = (page.parent / link.split("#", 1)[0]).resolve()
                            self.assertTrue(resource.is_relative_to(root.resolve()), (page, link))
                            self.assertTrue(resource.is_file(), (page, link))
                    if agent == "pi":
                        package = json.loads((root / "package.json").read_text())
                        self.assertIn("skills", package["pi"])
                        self.assertNotIn("extensions", package["pi"])
                        self.assertIn("package.json", manifest["files"])
                        self.assertNotIn("bundle.json", {name.casefold() for name in manifest["files"]})

    def test_portable_archives_are_deterministic_and_destinations_are_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            first = release.build("skills", "generic", base / "one")
            second = release.build("skills", "generic", base / "two")
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with self.assertRaisesRegex(ValueError, "must be new"):
                release.build("skills", "generic", base / "one")

    def test_inventory_detects_modified_unlisted_and_missing_files(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            archive = release.build("skills", "pi", base / "archive")
            root = qualify.unpack(archive, base / "extracted")
            readme = root / "README.md"
            original = readme.read_bytes()
            readme.write_text("changed")
            with self.assertRaisesRegex(ValueError, "integrity mismatch"):
                qualify.verify(root)
            readme.write_bytes(original)
            private = root / "PRIVATE.env"
            private.write_text("SYNTHETIC_PRIVATE")
            with self.assertRaisesRegex(ValueError, "inventory differs"):
                qualify.verify(root)
            private.unlink()
            readme.unlink()
            with self.assertRaisesRegex(ValueError, "inventory differs"):
                qualify.verify(root)

    def test_untrusted_archive_members_cannot_escape_or_link(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            for index, (name, kind) in enumerate((("../outside", tarfile.REGTYPE),
                                                 ("/outside", tarfile.REGTYPE),
                                                 ("autoresearch-toolkit/link", tarfile.SYMTYPE),
                                                 ("autoresearch-toolkit/link", tarfile.LNKTYPE))):
                archive = base / f"bad-{index}.tar.gz"
                with tarfile.open(archive, "w:gz") as tar:
                    member = tarfile.TarInfo(name)
                    member.type = kind
                    member.mode = 0o644
                    member.linkname = "/outside"
                    member.size = 1 if kind == tarfile.REGTYPE else 0
                    tar.addfile(member, io.BytesIO(b"x") if member.size else None)
                with self.assertRaisesRegex(ValueError, "unsafe"):
                    qualify.unpack(archive, base / str(index))
            self.assertFalse((base / "outside").exists())

    def test_profile_mismatches_fail_before_build_or_output_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "new"
            for profile, agent, target, pi in (("skills", "pi", None, True),
                                               ("engine", "codex", None, True),
                                               ("skills", "generic", "aarch64-apple-darwin", False)):
                with self.assertRaises(ValueError):
                    release.build(profile, agent, output, target, pi)
                self.assertFalse(output.exists())

    def test_version_manifests_remain_private_and_consistent(self):
        version = json.loads((ROOT / "package.json").read_text())["version"]
        for manifest in release.exporter.MANIFESTS.values():
            self.assertEqual(json.loads((ROOT / manifest).read_text())["version"], version)
        self.assertIn(f'version = "{version}"', (ROOT / "Cargo.toml").read_text())
        for name in ("package.json", "adapters/pi/package.json"):
            package = json.loads((ROOT / name).read_text())
            self.assertEqual(package["version"], version)
            self.assertTrue(package["private"])
        lock = json.loads((ROOT / "adapters/pi/package-lock.json").read_text())
        self.assertEqual(lock["version"], version)
        self.assertEqual(lock["packages"][""]["version"], version)


if __name__ == "__main__":
    unittest.main()
