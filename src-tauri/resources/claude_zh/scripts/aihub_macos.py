"""Pinned patch runner. No network access, shell interpolation, or account changes."""
import argparse
import json
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

# This script runs inside AIHub's signed resource bundle. Import caches would
# modify that bundle and invalidate its signature after the first installation.
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import patch_claude_zh_cn as patch


class Cancelled(Exception):
    pass


class ProgressReporter:
    def __init__(self, path):
        self.path = path
        self.critical = False

    def check(self):
        if not self.critical and self.path and self.path.with_name("cancel").exists():
            raise Cancelled()

    def write(self, phase, completed=None, total=None, interruptible=True):
        self.check()
        if not interruptible:
            self.critical = True
        if self.path:
            # Existing file is owned/readable by the requesting desktop user.
            self.path.write_text(json.dumps(dict(phase=phase, completed=completed,
                                                total=total, interruptible=interruptible)))

    def cancelled(self):
        if self.path:
            self.path.write_text(json.dumps(dict(phase="cancelled", interruptible=False)))


def assert_matching_backup(app):
    backups = patch.find_app_backups(app)
    if not backups or patch.bundle_version(patch.select_backup(backups)) != patch.bundle_version(app):
        raise RuntimeError("No matching-version backup")
    def build(path):
        with (path / "Contents/Info.plist").open("rb") as source:
            return plistlib.load(source).get("CFBundleVersion")
    if build(patch.select_backup(backups)) != build(app):
        raise RuntimeError("Backup belongs to a different build")


def run(app, home, action, reporter=None):
    reporter = reporter or ProgressReporter(None)
    old_progress, old_check = patch.progress, patch.check_cancel
    patch.progress, patch.check_cancel = reporter.write, reporter.check
    created = []
    original_mkdtemp = tempfile.mkdtemp

    def tracked_temp(*args, **kwargs):
        result = original_mkdtemp(*args, **kwargs)
        created.append(result)
        return result

    def quit_selected():
        pattern = "^" + re.escape(str(app / "Contents/MacOS")) + "/"
        if subprocess.run(["/usr/bin/pgrep", "-f", pattern], capture_output=True).returncode != 0:
            return
        escaped = str(app).replace("\\", "\\\\").replace('"', '\\"')
        subprocess.run(["/usr/bin/osascript", "-e", f'tell application "{escaped}" to quit'], check=True, capture_output=True)
        for _ in range(40):
            if subprocess.run(["/usr/bin/pgrep", "-f", pattern], capture_output=True).returncode != 0:
                return
            time.sleep(.25)
        raise RuntimeError("Claude did not exit; save your work and close it")

    tempfile.mkdtemp = tracked_temp
    patch.quit_claude = quit_selected
    try:
        reporter.write("preparing")
        if action == "restore":
            assert_matching_backup(app)
            sys.argv = ["patch", "--app", str(app), "--user-home", str(home), "--restore"]
        else:
            if not patch.is_officially_signed(app):
                assert_matching_backup(app)
                quit_selected()
                reporter.write("restoring", interruptible=False)
                patch.restore_backup(app, False)
                reporter.critical = False
                reporter.check()
            sys.argv = ["patch", "--app", str(app), "--user-home", str(home), "--lang", "zh-CN"]
        result = patch.main()
        reporter.critical = True
        reporter.write("completed", interruptible=False)
        return result
    finally:
        patch.progress, patch.check_cancel = old_progress, old_check
        tempfile.mkdtemp = original_mkdtemp
        for path in reversed(created):
            shutil.rmtree(path, ignore_errors=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["install", "restore"])
    parser.add_argument("app", type=Path)
    parser.add_argument("home", type=Path)
    parser.add_argument("progress", type=Path, nargs="?")
    args = parser.parse_args()
    reporter = ProgressReporter(args.progress)
    try:
        raise SystemExit(run(args.app, args.home, args.action, reporter))
    except Cancelled:
        reporter.cancelled()
        raise SystemExit(130)
    except Exception:
        # Native caller reports an actionable fixed message, never raw command output.
        raise SystemExit(1)
