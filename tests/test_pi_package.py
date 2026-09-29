import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import package_pi
import qualify


@unittest.skipUnless(shutil.which("npm"), "npm is required for tarball tests")
class PiPackageTests(unittest.TestCase):
    def test_real_tarball_has_complete_resources_and_excludes_unlisted_files(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            source = base / "source"
            for name in {*package_pi.FILES, package_pi.TEMPLATE, package_pi.README,
                         "package.json", "adapters/pi/package.json"}:
                target = source / name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / name, target)
            for name in ("skills/autoresearch-scout/.env", "adapters/pi/credentials.json",
                         "skills/autoresearch-run/references/PRIVATE.md"):
                (source / name).write_text("SYNTHETIC_PRIVATE_SENTINEL")
            with patch.object(package_pi, "ROOT", source), patch.object(package_pi.exporter, "ROOT", source):
                archive = package_pi.build(base / "npm")
            root = qualify.unpack(archive, base / "unpacked", root_name="package")
            manifest = package_pi.verify(root)
            self.assertEqual(set(manifest["files"]), {*package_pi.FILES, "package.json"})
            self.assertTrue(all(b"SYNTHETIC_PRIVATE_SENTINEL" not in (root / name).read_bytes()
                                for name in manifest["files"]))
            self.assertFalse((root / "node_modules").exists())
            self.assertFalse((root / "adapters/pi/package.json").exists())
            package = json.loads((root / "package.json").read_text())
            self.assertEqual(package["name"], "@metimer/ultramarine")
            self.assertNotIn("private", package)
            self.assertEqual(package["publishConfig"]["access"], "public")
            self.assertEqual(qualify.qualify(root)["portable_helper"], "passed")
            (root / "adapters/pi/index.ts").write_text("tampered")
            with self.assertRaisesRegex(ValueError, "integrity mismatch"):
                package_pi.verify(root)

    def test_final_build_refuses_dirty_sources_without_creating_output(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "npm"
            with patch.object(package_pi.release, "run", side_effect=["a" * 40, " M README.md"]):
                with self.assertRaisesRegex(ValueError, "commit the final changes"):
                    package_pi.build(output, require_clean=True)
            self.assertFalse(output.exists())

    def test_source_and_template_remain_private_and_versions_match(self):
        source = json.loads((ROOT / "package.json").read_text())
        template = json.loads((ROOT / package_pi.TEMPLATE).read_text())
        self.assertTrue(source["private"])
        self.assertTrue(template["private"])
        self.assertEqual(source["version"], template["version"])


if __name__ == "__main__":
    unittest.main()
