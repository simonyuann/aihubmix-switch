"""Attach the local image CLI to the selected Switch provider, without secrets here.

Codex supplies this directory through its own shell environment policy.  Other
Python invocations keep their normal sitecustomize behaviour.
"""
import importlib.machinery
import json
import os
from pathlib import Path
import sys


def _run_original_sitecustomize():
    here = Path(__file__).resolve().parent
    paths = []
    for value in sys.path:
        try:
            if Path(value or os.curdir).resolve() == here:
                continue
        except (OSError, ValueError):
            pass
        paths.append(value)
    spec = importlib.machinery.PathFinder.find_spec("sitecustomize", paths)
    if spec is not None and spec.loader is not None:
        module = sys.modules[__name__]
        module.__file__ = spec.origin
        module.__loader__ = spec.loader
        module.__spec__ = spec
        module.__package__ = spec.parent
        spec.loader.exec_module(module)


def _start():
    # Never give credentials to arbitrary scripts with a similar name.
    script = Path(sys.argv[0]) if sys.argv else Path()
    if script.name != "image_gen.py":
        _run_original_sitecustomize()
        return

    here = Path(__file__).resolve().parent
    try:
        config = json.loads((here / "config.json").read_text(encoding="utf-8"))
        expected = Path(config["codex_home"]) / "skills" / ".system" / "imagegen" / "scripts" / "image_gen.py"
        if script.resolve() != expected.resolve():
            _run_original_sitecustomize()
            return
        helper = here / "aihubmix_imagegen.py"
        command = [sys.executable, str(helper), "--data-dir", config["data_dir"],
                   "--device-dir", config["device_dir"], "--", sys.executable, *sys.argv]
        if os.name == "nt":
            # Replacing an interpreter from sitecustomize with execve is not
            # reliable on Windows. Wait for the guarded wrapper instead and
            # stop this interpreter before the original script can continue.
            import subprocess
            result = subprocess.call(command, env=os.environ.copy())
            os._exit(result if 0 <= result <= 255 else 1)
        os.execve(sys.executable, command, os.environ.copy())
    except BaseException:
        # Python normally ignores sitecustomize errors and continues executing
        # the script.  That could expose inherited stale credentials.
        try:
            sys.stderr.write("无法安全启动生图：请检查 AIHubMix Switch 的生图接入设置。\n")
            sys.stderr.flush()
        finally:
            os._exit(2)


_start()
