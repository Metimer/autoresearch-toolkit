import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import prepare_publication
import qualify
import release


class PublicationTests(unittest.TestCase):
    def fixture(self, base):
        commit = "a" * 40
        # Synthetic provenance is restricted to this temporary test artifact.
        with patch.object(release, "run", side_effect=[commit, ""]):
            archive = release.build("skills", "pi", base / "candidate-skills-pi")
        root = qualify.unpack(archive, base / "extracted")
        report = qualify.qualify(root)
        report["archive_sha256"] = qualify.sha(archive)
        (archive.parent / "qualification.json").write_text(json.dumps(report))
        return archive, commit, report

    def test_matching_archive_and_report_are_accepted_but_other_commits_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            archive, commit, _ = self.fixture(Path(directory))
            self.assertEqual(prepare_publication.inspect(archive.parent, commit, "skills", "pi", None), archive)
            with self.assertRaisesRegex(ValueError, "different source revision"):
                prepare_publication.inspect(archive.parent, "b" * 40, "skills", "pi", None)

    def test_modified_archive_and_stale_report_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            archive, commit, report = self.fixture(Path(directory))
            report["archive_sha256"] = "0" * 64
            (archive.parent / "qualification.json").write_text(json.dumps(report))
            with self.assertRaisesRegex(ValueError, "qualification does not match"):
                prepare_publication.inspect(archive.parent, commit, "skills", "pi", None)
            with archive.open("ab") as stream:
                stream.write(b"tampered")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                prepare_publication.inspect(archive.parent, commit, "skills", "pi", None)

    def test_incomplete_matrix_never_creates_publication_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            _, commit, _ = self.fixture(base)
            output = base / "publication"
            with self.assertRaisesRegex(ValueError, "expected one archive"):
                prepare_publication.collect(base, output, commit)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
