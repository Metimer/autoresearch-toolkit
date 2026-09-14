from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]
MEASURE_PATH = ROOT / "skills/autoresearch-scout/scripts/measure.py"


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


measure = module("measure", MEASURE_PATH)
exporter = module("exporter", ROOT / "scripts/export.py")


class MeasurementTests(unittest.TestCase):
    def invoke(self, options, code):
        with tempfile.TemporaryDirectory() as directory:
            return subprocess.run(
                [sys.executable, str(MEASURE_PATH), *options, "--",
                 sys.executable, "-c", code],
                cwd=directory, capture_output=True, text=True, timeout=10,
            )

    def test_success_metrics_are_finite_and_diagnostics_are_separate(self):
        result = self.invoke(["--runs", "3", "--warmup", "1"], "print('workload')")
        self.assertEqual(result.returncode, 0, result.stderr)
        values = dict(line.removeprefix("METRIC ").split("=")
                      for line in result.stdout.splitlines())
        self.assertEqual(set(values), {"bench_ms", "run_min_ms", "run_max_ms"})
        self.assertTrue(all(math.isfinite(float(v)) and float(v) > 0
                            for v in values.values()))
        self.assertLessEqual(float(values["run_min_ms"]), float(values["bench_ms"]))
        self.assertLessEqual(float(values["bench_ms"]), float(values["run_max_ms"]))
        self.assertNotIn("workload", result.stdout)
        self.assertEqual(result.stderr.count("workload"), 4)

    def test_warmup_excluded_and_median_not_mean(self):
        output = io.StringIO()
        with patch.object(measure, "sample", side_effect=[999, 1, 9, 2]) as run:
            with contextlib.redirect_stdout(output):
                status = measure.main(["--runs", "3", "--warmup", "1", "--", "x"])
        self.assertEqual(status, 0)
        self.assertEqual(run.call_count, 4)
        self.assertIn("METRIC bench_ms=2.000000", output.getvalue())

    def test_failed_workload_has_no_metrics(self):
        for warmup in ("0", "1"):
            with self.subTest(warmup=warmup):
                result = self.invoke(["--runs", "1", "--warmup", warmup],
                                     "print('bad'); raise SystemExit(7)")
                self.assertEqual(result.returncode, 1)
                self.assertEqual(result.stdout, "")
                self.assertIn("MEASUREMENT FAILED", result.stderr)

    def test_single_run_timeout(self):
        result = self.invoke(
            ["--runs", "1", "--warmup", "0", "--timeout", "0.05"],
            "import time; time.sleep(10)",
        )
        self.assertEqual(result.returncode, 124)
        self.assertEqual(result.stdout, "")

    def test_total_budget_covers_warmup_too(self):
        result = self.invoke(
            ["--runs", "5", "--warmup", "1", "--timeout", "5", "--budget", "0.05"],
            "import time; time.sleep(10)",
        )
        self.assertEqual(result.returncode, 124)
        self.assertEqual(result.stdout, "")

    def test_budget_exhausted_before_next_sample(self):
        with patch.object(measure.time, "monotonic", side_effect=[0, 2]):
            with patch.object(measure, "sample") as run:
                with contextlib.redirect_stderr(io.StringIO()):
                    status = measure.main(["--budget", "1", "--", "x"])
        self.assertEqual(status, 124)
        run.assert_not_called()

    def test_invalid_arguments_fail_without_execution(self):
        cases = [
            ["--runs", "0"], ["--runs", "32"], ["--warmup", "-1"],
            ["--timeout", "nan"], ["--timeout", "inf"], ["--timeout", "0"],
            ["--timeout", "601"], ["--budget", "3601"], ["--name", "a=b"],
            ["--name", "run_min_ms"], ["--name", "run_max_ms"],
        ]
        for options in cases:
            with self.subTest(options=options):
                result = self.invoke(options, "print('must-not-run')")
                self.assertEqual(result.returncode, 2)
                self.assertEqual(result.stdout, "")
                self.assertNotIn("must-not-run", result.stderr)

    def test_missing_executable_fails_cleanly(self):
        result = subprocess.run(
            [sys.executable, str(MEASURE_PATH), "--", "/missing-autoresearch-command"],
            capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertNotIn("Traceback", result.stderr)

    def test_no_implicit_shell_interpolation(self):
        result = self.invoke(["--runs", "1", "--warmup", "0"],
                             "print('literal $TOKEN ; $(echo x)')")
        self.assertEqual(result.returncode, 0)
        self.assertIn("literal $TOKEN ; $(echo x)", result.stderr)

    def test_timeout_kills_owned_process_group(self):
        fake = Mock(pid=1234)
        fake.wait.side_effect = [subprocess.TimeoutExpired(["work"], 1), 0]
        with patch.object(measure.subprocess, "Popen", return_value=fake):
            with patch.object(measure.os, "killpg") as kill:
                with self.assertRaises(subprocess.TimeoutExpired):
                    measure.sample(["work"], 1)
        kill.assert_called_once_with(1234, measure.signal.SIGKILL)

    def test_keyboard_interrupt_kills_owned_group(self):
        fake = Mock(pid=1234)
        fake.wait.side_effect = [KeyboardInterrupt(), 0]
        with patch.object(measure.subprocess, "Popen", return_value=fake):
            with patch.object(measure.os, "killpg") as kill:
                with self.assertRaises(KeyboardInterrupt):
                    measure.sample(["work"], 1)
        kill.assert_called_once_with(1234, measure.signal.SIGKILL)


class ExportTests(unittest.TestCase):
    def fixture(self, base):
        source = base / "source"
        for name in (*exporter.PORTABLE_FILES, *exporter.MANIFESTS.values()):
            target = source / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, target)
        return source

    def test_unlisted_private_files_are_excluded_from_every_adapter(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            source = self.fixture(base)
            private_files = (
                ".env", ".env.production", ".auto/session.jsonl", ".npmrc",
                "assets/private.key", "assets/customer-data.csv",
                "scripts/credentials.json", "node_modules/private/index.js",
            )
            for name in private_files:
                file = source / "skills/autoresearch-scout" / name
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text("SYNTHETIC_PRIVATE_SENTINEL")
            with patch.object(exporter, "ROOT", source):
                for agent, manifest in exporter.MANIFESTS.items():
                    with self.subTest(agent=agent):
                        target = exporter.export(agent, base / agent / "autoresearch-toolkit")
                        files = {str(p.relative_to(target)) for p in target.rglob("*")
                                 if p.is_file()}
                        self.assertEqual(files, {*exporter.PORTABLE_FILES, manifest})
                        self.assertTrue(all(b"SYNTHETIC_PRIVATE_SENTINEL" not in
                                            (target / name).read_bytes() for name in files))

    def test_symlinked_manifest_parent_rejected_before_output_creation(self):
        for agent in ("codex", "claude", "cursor"):
            with self.subTest(agent=agent), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                source = self.fixture(base)
                parent = (source / exporter.MANIFESTS[agent]).parent
                outside = base / "external"
                parent.rename(outside)
                parent.symlink_to(outside, target_is_directory=True)
                target = base / "output/autoresearch-toolkit"
                with patch.object(exporter, "ROOT", source):
                    with self.assertRaisesRegex(ValueError, "symlinked resource"):
                        exporter.export(agent, target)
                self.assertFalse(target.parent.exists())

    def test_symlinked_resources_and_ancestors_rejected(self):
        paths = (
            "README.md", "PROVENANCE.md", "LICENSE", "skills",
            "skills/autoresearch-scout", "skills/autoresearch-scout/scripts",
            "skills/autoresearch-scout/scripts/measure.py",
            *exporter.MANIFESTS.values(),
        )
        for name in paths:
            with self.subTest(path=name), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                source = self.fixture(base)
                resource = source / name
                outside = base / "external"
                is_directory = resource.is_dir()
                resource.rename(outside)
                resource.symlink_to(outside, target_is_directory=is_directory)
                agent = next((a for a, m in exporter.MANIFESTS.items() if m == name),
                             "codex")
                target = base / "output/autoresearch-toolkit"
                with patch.object(exporter, "ROOT", source):
                    with self.assertRaisesRegex(ValueError, "symlinked resource"):
                        exporter.export(agent, target)
                self.assertFalse(target.parent.exists())

    def test_missing_or_directory_resource_rejected_before_output_creation(self):
        for kind in ("missing", "directory", "dangling_symlink"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                source = self.fixture(base)
                resource = source / "skills/autoresearch-scout/assets/prompt.md"
                resource.unlink()
                if kind == "directory":
                    resource.mkdir()
                elif kind == "dangling_symlink":
                    resource.symlink_to(base / "missing")
                target = base / "output/autoresearch-toolkit"
                with patch.object(exporter, "ROOT", source):
                    with self.assertRaises(ValueError):
                        exporter.export("codex", target)
                self.assertFalse(target.parent.exists())

    def test_output_inside_source_skills_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            source = self.fixture(Path(directory))
            target = source / "skills/output/autoresearch-toolkit"
            with patch.object(exporter, "ROOT", source):
                with self.assertRaisesRegex(ValueError, "outside the source skills"):
                    exporter.export("codex", target)
            self.assertFalse(target.parent.exists())

    def test_each_adapter_is_self_contained_without_originals(self):
        with tempfile.TemporaryDirectory() as directory:
            for agent, manifest in exporter.MANIFESTS.items():
                with self.subTest(agent=agent):
                    target = Path(directory) / agent / "autoresearch-toolkit"
                    exporter.export(agent, target)
                    data = json.loads((target / manifest).read_text())
                    self.assertEqual(data["name"], target.name)
                    self.assertEqual(data["version"], "0.1.0")
                    self.assertFalse((target / "originals").exists())
                    self.assertFalse((target / ".auto").exists())
                    self.assertEqual((target / "LICENSE").read_bytes(),
                                     (ROOT / "LICENSE").read_bytes())
                    for source in (ROOT / "skills").rglob("*"):
                        if source.is_file() and "__pycache__" not in source.parts:
                            self.assertEqual(
                                source.read_bytes(),
                                (target / source.relative_to(ROOT)).read_bytes(),
                            )
                    helper = target / MEASURE_PATH.relative_to(ROOT)
                    result = subprocess.run(
                        [sys.executable, str(helper), "--runs", "1", "--warmup", "0",
                         "--", sys.executable, "-c", "pass"],
                        cwd=directory, capture_output=True, text=True, timeout=10,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)

    def test_existing_output_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "autoresearch-toolkit"
            target.mkdir()
            marker = target / "user-file"
            marker.write_text("keep me")
            with self.assertRaises(FileExistsError):
                exporter.export("codex", target)
            self.assertEqual(marker.read_text(), "keep me")
            self.assertEqual(list(target.iterdir()), [marker])

    def test_dangling_destination_symlink_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "autoresearch-toolkit"
            missing = Path(directory) / "missing"
            target.symlink_to(missing, target_is_directory=True)
            with self.assertRaises(FileExistsError):
                exporter.export("codex", target)
            self.assertFalse(missing.exists())

    def test_missing_resource_does_not_create_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "export/autoresearch-toolkit"
            with patch.object(exporter, "ROOT", Path(directory) / "absent"):
                with self.assertRaises(ValueError):
                    exporter.export("pi", target)
            self.assertFalse(target.exists())

    def test_wrong_plugin_folder_name_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "wrong"
            with self.assertRaises(ValueError):
                exporter.export("codex", target)
            self.assertFalse(target.exists())

    def test_pi_portable_adapter_does_not_load_original_engine(self):
        manifest = json.loads((ROOT / "package.json").read_text())
        self.assertEqual(manifest["pi"], {"skills": ["./skills"]})
        self.assertTrue(manifest["private"])


if __name__ == "__main__":
    unittest.main()
