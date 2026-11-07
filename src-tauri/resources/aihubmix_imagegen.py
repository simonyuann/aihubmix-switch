#!/usr/bin/env python3
"""Run the Codex image_gen.py CLI with the currently selected provider.

The key and URL are read together from one provider row for each invocation.
This helper never persists them or places them in a command line.
"""
import argparse
from contextlib import closing
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import sys
import threading
from urllib.parse import quote, urlsplit, urlunsplit

try:
    import tomllib
except ImportError:
    sys.stderr.write("AIHubMix 生图包装器需要 Python 3.11 或更新版本。\n")
    raise SystemExit(2)


class Unavailable(Exception):
    pass


def _optional_bytes(path):
    try:
        return path.read_bytes()
    except FileNotFoundError:
        return b""


def _json_object(data):
    value = json.loads(data) if data else {}
    if not isinstance(value, dict):
        raise Unavailable()
    return value


def _selection(settings_bytes, state_bytes):
    settings = _json_object(settings_bytes)
    state = _json_object(state_bytes)
    apps = state.get("apps", {})
    if not isinstance(apps, dict) or not isinstance(apps.get("codex", {}), dict):
        raise Unavailable()
    codex = apps.get("codex", {})
    if codex.get("pending") is not None:
        raise Unavailable()
    mode = codex.get("mode")
    if mode not in (None, "direct", "proxy"):
        raise Unavailable()
    stack = codex.get("stack", {})
    if mode == "proxy" and (not isinstance(stack, dict) or stack.get("enabled")):
        raise Unavailable()
    selected = codex.get("proxy_route") if mode == "proxy" else settings.get("currentProviderCodex")
    if mode == "proxy" and not selected:
        raise Unavailable()
    if selected is not None and (not isinstance(selected, str) or not selected.strip()):
        raise Unavailable()
    return selected, mode == "proxy"


def _read_current_provider(data_dir, device_dir):
    db = (data_dir / "cc-switch.db").resolve()
    if not db.is_file():
        raise Unavailable()
    settings_path = device_dir / "settings.json"
    state_path = device_dir / "live-state.json"
    for _ in range(3):
        before = (_optional_bytes(settings_path), _optional_bytes(state_path))
        selected, proxy = _selection(*before)
        with closing(sqlite3.connect(db.as_uri() + "?mode=ro", uri=True, timeout=3)) as conn:
            conn.execute("BEGIN")
            if proxy:
                failover = conn.execute(
                    "SELECT auto_failover_enabled FROM proxy_config WHERE app_type = ?", ("codex",)
                ).fetchone()
                if failover and failover[0]:
                    raise Unavailable()
            if selected:
                rows = conn.execute(
                    "SELECT settings_config, category, meta FROM providers WHERE app_type = ? AND id = ?",
                    ("codex", selected),
                ).fetchall()
            else:
                rows = conn.execute(
                    "SELECT settings_config, category, meta FROM providers WHERE app_type = ? AND is_current = 1",
                    ("codex",),
                ).fetchall()
        after = (_optional_bytes(settings_path), _optional_bytes(state_path))
        if before == after:
            if len(rows) != 1:
                raise Unavailable()
            return rows[0]
    raise Unavailable()


def _base_url(value):
    if not isinstance(value, str) or value != value.strip() or not value:
        raise Unavailable()
    parsed = urlsplit(value)
    if (parsed.scheme not in ("http", "https") or not parsed.hostname or
            parsed.username is not None or parsed.password is not None or
            parsed.query or parsed.fragment or any(c.isspace() for c in value)):
        raise Unavailable()
    # Force validation of the optional port before giving the value to the SDK.
    _ = parsed.port
    path = parsed.path.rstrip("/")
    path = re.sub(r"/(?:responses(?:/compact)?|chat/completions|images/(?:generations|edits))$", "", path, flags=re.I)
    if not path:
        path = "/v1"
    elif not path.endswith("/v1"):
        # A non-root custom prefix may itself be an API root. Do not guess a
        # different endpoint and send credentials there.
        raise Unavailable()
    return urlunsplit((parsed.scheme, parsed.netloc, path, "", ""))


def resolve_pair(data_dir, device_dir):
    settings_text, category, meta_text = _read_current_provider(Path(data_dir), Path(device_dir))
    settings = _json_object(settings_text)
    meta = _json_object(meta_text)
    binding = meta.get("authBinding")
    if (category == "official" or meta.get("providerType") in
            ("codex_oauth", "github_copilot", "xai_oauth") or
            (isinstance(binding, dict) and binding.get("source") == "managed_account") or
            meta.get("apiFormat") in ("anthropic", "gemini_native")):
        raise Unavailable()
    config = settings.get("config")
    if not isinstance(config, str):
        raise Unavailable()
    parsed = tomllib.loads(config)
    active = parsed.get("model_provider")
    if active is not None:
        providers = parsed.get("model_providers", {})
        if not isinstance(active, str) or not isinstance(providers, dict):
            raise Unavailable()
        table = providers.get(active)
        if not isinstance(table, dict):
            raise Unavailable()
        base = table.get("base_url")
        bearer = table.get("experimental_bearer_token") or parsed.get("experimental_bearer_token")
    else:
        base = parsed.get("base_url")
        bearer = parsed.get("experimental_bearer_token")
    auth = settings.get("auth")
    key = auth.get("OPENAI_API_KEY") if isinstance(auth, dict) else None
    key = key or bearer
    if not isinstance(key, str) or not key.strip() or any(c in key for c in "\r\n\0"):
        raise Unavailable()
    key = key.strip()
    if key in ("***", "PROXY_MANAGED", "codex_oauth_placeholder", "xai_oauth_placeholder", "copilot_placeholder"):
        raise Unavailable()
    return key, _base_url(base)


def _copy_redacted(source, destination, secrets):
    for line in iter(source.readline, b""):
        try:
            for secret in secrets:
                line = line.replace(secret, b"[REDACTED]")
            destination.write(line)
            destination.flush()
        except (BrokenPipeError, OSError):
            break


def _stop_if_provider_changes(child, data_dir, device_dir, pair, done, changed):
    while not done.wait(0.25):
        if child.poll() is not None:
            return
        try:
            still_current = resolve_pair(data_dir, device_dir) == pair
        except (Unavailable, OSError, ValueError, TypeError, sqlite3.Error):
            still_current = False
        if not still_current:
            try:
                child.terminate()
                changed.set()
            except OSError:
                pass
            return


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise Unavailable()


def main(argv=None):
    parser = Parser(add_help=False)
    parser.add_argument("--data-dir", required=True)
    parser.add_argument("--device-dir", required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    try:
        args = parser.parse_args(argv)
        command = args.command[1:] if args.command[:1] == ["--"] else args.command
        executable = Path(command[0]).name.lower() if command else ""
        if (len(command) < 2 or not re.fullmatch(r"python(?:3(?:\.[0-9]+)?)?(?:\.exe)?", executable)
                or Path(command[1]).name != "image_gen.py"):
            raise Unavailable()
        key, base = resolve_pair(args.data_dir, args.device_dir)
        env = os.environ.copy()
        for name in ("OPENAI_API_KEY", "OPENAI_BASE_URL", "OPENAI_ORG_ID", "OPENAI_PROJECT_ID", "OPENAI_ORGANIZATION", "OPENAI_PROJECT"):
            env.pop(name, None)
        # The automatic hook has already selected this wrapper.  Remove only
        # its own directory so the real image script does not wrap recursively;
        # leave any existing Python paths and sitecustomize intact.
        hook_dir = Path(__file__).resolve().parent
        if (hook_dir / "sitecustomize.py").is_file() and "PYTHONPATH" in env:
            paths = env["PYTHONPATH"].split(os.pathsep)
            paths = [path for path in paths if not path or Path(path).resolve() != hook_dir]
            if paths:
                env["PYTHONPATH"] = os.pathsep.join(paths)
            else:
                env.pop("PYTHONPATH", None)
        env.update(OPENAI_API_KEY=key, OPENAI_BASE_URL=base)
        secrets = sorted({key.encode("utf-8"), quote(key, safe="").encode("ascii"),
                          json.dumps(key, ensure_ascii=True)[1:-1].encode("ascii")}, key=len, reverse=True)
        child = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        done, changed = threading.Event(), threading.Event()
        monitor = threading.Thread(target=_stop_if_provider_changes,
                                   args=(child, args.data_dir, args.device_dir, (key, base), done, changed))
        threads = [threading.Thread(target=_copy_redacted, args=(child.stdout, sys.stdout.buffer, secrets)),
                   threading.Thread(target=_copy_redacted, args=(child.stderr, sys.stderr.buffer, secrets))]
        monitor.start()
        for thread in threads:
            thread.start()
        result = child.wait()
        done.set()
        monitor.join()
        for thread in threads:
            thread.join()
        if changed.is_set():
            sys.stderr.write("服务商或密钥已变化，生图进程已停止；已发出的请求无法撤回。\n")
        return result
    except (Unavailable, OSError, ValueError, TypeError, sqlite3.Error, json.JSONDecodeError):
        sys.stderr.write("无法安全启动生图：请确认当前选择的是已配置的第三方 OpenAI 兼容服务商，并使用 image_gen.py。\n")
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
