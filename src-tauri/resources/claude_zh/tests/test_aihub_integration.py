import importlib.util
import json
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("aihub_macos", ROOT / "scripts/aihub_macos.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class IntegrationTests(unittest.TestCase):
    def test_entrypoint_reports_installation_without_writing_into_signed_bundle(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            script = root / "aihub_macos.py"
            shutil.copyfile(ROOT / "scripts/aihub_macos.py", script)
            (root / "patch_claude_zh_cn.py").write_text(
                "def is_officially_signed(app): return True\ndef main(): return 0\ndef progress(*a, **k): pass\ndef check_cancel(): pass\n"
            )
            marker = root / "phase"
            result = subprocess.run(
                [sys.executable, "-I", "-S", str(script), "install", str(root / "Claude.app"), str(root), str(marker)],
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            self.assertEqual(json.loads(marker.read_text())["phase"], "completed")
            self.assertFalse((root / "__pycache__").exists())

    def test_cancellation_stops_at_boundary_and_defers_during_replacement(self):
        with tempfile.TemporaryDirectory() as root:
            marker = Path(root) / "phase"
            reporter = runner.ProgressReporter(marker)
            reporter.write("translating", 5, 10)
            self.assertEqual(json.loads(marker.read_text())["completed"], 5)
            marker.with_name("cancel").touch()
            with self.assertRaises(runner.Cancelled):
                reporter.write("signing")
            marker.with_name("cancel").unlink()
            reporter.write("replacing", interruptible=False)
            marker.with_name("cancel").touch()
            reporter.write("completed", interruptible=False)
            self.assertEqual(json.loads(marker.read_text())["phase"], "completed")

    def test_cancellation_cleans_working_copy_and_restores_callbacks(self):
        with tempfile.TemporaryDirectory() as root:
            marker = Path(root) / "phase"
            reporter = runner.ProgressReporter(marker)
            created = []
            original = runner.patch.progress
            def work():
                created.append(Path(tempfile.mkdtemp(dir=root)))
                marker.with_name("cancel").touch()
                runner.patch.progress("translating")
            with patch.object(runner.patch, "is_officially_signed", return_value=True), patch.object(runner.patch, "main", side_effect=work):
                with self.assertRaises(runner.Cancelled):
                    runner.run(Path(root) / "Claude.app", Path(root), "install", reporter)
            self.assertFalse(created[0].exists())
            self.assertIs(runner.patch.progress, original)

    def test_parallel_progress_and_cancellation_join_owned_workers(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            harness = root / "worker_test.py"
            harness.write_text("""
import multiprocessing
import sys
from pathlib import Path
sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
import patch_claude_zh_cn as patch
import aihub_macos as runner
if __name__ == '__main__':
    root = Path(sys.argv[2])
    for index in range(65):
        (root / f'{index}.js').write_text('const x = "Hello World";')
    patch.find_frontend_assets_dir = lambda app: root
    patch.load_frontend_hardcoded_replacements = lambda lang: [('Hello World', '你好')]
    marker = root / 'phase'
    reporter = runner.ProgressReporter(marker)
    updates = []
    def update(phase, completed=None, total=None, interruptible=True):
        reporter.write(phase, completed, total, interruptible)
        updates.append(completed)
        if sys.argv[3] == 'cancel' and completed:
            marker.with_name('cancel').touch()
    patch.progress = update
    patch.check_cancel = reporter.check
    try:
        patch.patch_hardcoded_frontend_strings(root, 'zh-CN')
        assert sys.argv[3] == 'complete'
        assert updates[-1] == 65
    except runner.Cancelled:
        assert sys.argv[3] == 'cancel'
    assert not multiprocessing.active_children()
""")
            for mode in ["complete", "cancel"]:
                result = subprocess.run([sys.executable, "-B", "-I", "-S", str(harness),
                    str(ROOT / "scripts"), str(root), mode], capture_output=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr.decode())

    def test_replacement_failure_rolls_back_original(self):
        with tempfile.TemporaryDirectory() as root:
            original = Path(root) / "Claude.app"
            replacement = Path(root) / "patched.app"
            original.mkdir()
            (original / "original").write_text("preserve")
            replacement.mkdir()
            move = shutil.move
            def fail_copy(src, dst):
                if src == str(replacement):
                    Path(dst).mkdir()
                    raise OSError("synthetic disk failure")
                return move(src, dst)
            with patch.object(runner.patch.shutil, "move", side_effect=fail_copy):
                with self.assertRaises(OSError):
                    runner.patch.backup_and_replace(original, replacement, False)
            self.assertEqual((original / "original").read_text(), "preserve")

    def test_restore_refuses_cross_version_backups(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            app = root / "Claude.app"
            backup = root / "Claude.backup-before-zh-CN-20260101.app"
            for path, version in [(app, "4.2"), (backup, "4.1")]:
                (path / "Contents").mkdir(parents=True)
                (path / "Contents/Info.plist").write_bytes(plistlib.dumps({"CFBundleShortVersionString": version}))
            with self.assertRaises(RuntimeError):
                runner.assert_matching_backup(app)
            self.assertTrue(app.exists())
            (backup / "Contents/Info.plist").write_bytes((app / "Contents/Info.plist").read_bytes())
            runner.assert_matching_backup(app)
            (app / "Contents/Info.plist").write_bytes(plistlib.dumps({"CFBundleShortVersionString": "4.2", "CFBundleVersion": "new-build"}))
            with self.assertRaises(RuntimeError):
                runner.assert_matching_backup(app)

    def test_patch_failure_cleans_only_created_temporary_directories(self):
        with tempfile.TemporaryDirectory() as root:
            owned = []
            def fail():
                owned.append(Path(tempfile.mkdtemp(dir=root)))
                raise RuntimeError("synthetic failure")
            with patch.object(runner.patch, "is_officially_signed", return_value=True), patch.object(runner.patch, "main", side_effect=fail):
                with self.assertRaises(RuntimeError):
                    runner.run(Path(root) / "Claude.app", Path(root), "install")
            self.assertFalse(owned[0].exists())
            self.assertTrue(Path(root).exists())

    def test_windows_complete_patch_uses_one_version_bound_backup(self):
        script = (ROOT / "scripts/install_windows.ps1").read_text(encoding="utf-8-sig")
        self.assertIn('".aihub-zh-backups-$safeVersion"', script)
        self.assertNotIn('Join-Path $ResourcesPath ".zh-cn-backups"', script)
        self.assertIn('Backup-ModifiedFile $ResourcesPath $svcPath', script)
        self.assertIn('if ($coworkResult -match', script)
        self.assertIn('$env:CLAUDE_ZH_SKIP_UPDATE_CHECK = "1"', script)

if __name__ == "__main__":
    unittest.main()
