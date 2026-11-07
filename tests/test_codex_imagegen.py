import importlib.util
from contextlib import closing
import json
import os
from pathlib import Path
import shutil
import shlex
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


HELPER = Path(__file__).resolve().parents[1] / "src-tauri/resources/aihubmix_imagegen.py"
spec = importlib.util.spec_from_file_location("aihubmix_imagegen", HELPER)
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


class Capture(BaseHTTPRequestHandler):
    requests = []

    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", "0")))
        self.requests.append((self.path, self.headers.get("Authorization")))
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"{}")

    def log_message(self, *_args):
        pass


class CodexImagegenTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), Capture)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.data = self.root / "data"
        self.device = self.root / "device"
        self.data.mkdir()
        self.device.mkdir()
        self.db = self.data / "cc-switch.db"
        with closing(sqlite3.connect(self.db)) as conn:
            conn.execute("CREATE TABLE providers (id TEXT, app_type TEXT, settings_config TEXT, category TEXT, meta TEXT, is_current INTEGER)")
            conn.execute("CREATE TABLE proxy_config (app_type TEXT, auto_failover_enabled INTEGER)")
            conn.commit()
        self.script = self.root / "image_gen.py"
        self.script.write_text(
            "import json, os, sys, time, urllib.request\n"
            "from pathlib import Path\n"
            "if len(sys.argv) > 2 and sys.argv[1] == 'wait':\n"
            "    Path(sys.argv[2]).write_text('ready')\n"
            "    time.sleep(5)\n"
            "key = os.environ['OPENAI_API_KEY']\n"
            "assert 'OPENAI_PROJECT_ID' not in os.environ\n"
            "req = urllib.request.Request(os.environ['OPENAI_BASE_URL'] + '/images/generations', "
            "data=b'{}', headers={'Authorization': 'Bearer ' + key}, method='POST')\n"
            "urllib.request.urlopen(req).read()\n"
            "print('result ' + key)\n"
            "print(json.dumps(key))\n"
            "print('error ' + key, file=sys.stderr)\n",
            encoding="utf-8",
        )
        Capture.requests.clear()

    def save(self, provider_id, base, key, category="custom", meta=None):
        config = f'model_provider = "custom"\n[model_providers.custom]\nbase_url = "{base}"\n'
        settings = {"auth": {"OPENAI_API_KEY": key}, "config": config}
        with closing(sqlite3.connect(self.db)) as conn:
            conn.execute("DELETE FROM providers WHERE id = ?", (provider_id,))
            conn.execute("INSERT INTO providers VALUES (?, 'codex', ?, ?, ?, 0)",
                         (provider_id, json.dumps(settings), category, json.dumps(meta or {})))
            conn.commit()

    def select(self, provider_id, state=None):
        (self.device / "settings.json").write_text(json.dumps({"currentProviderCodex": provider_id}), encoding="utf-8")
        (self.device / "live-state.json").write_text(json.dumps(state or {}), encoding="utf-8")

    def run_helper(self):
        env = os.environ.copy()
        env.update(OPENAI_API_KEY="stale-secret", OPENAI_BASE_URL="https://stale.invalid/v1",
                   OPENAI_PROJECT_ID="old-project")
        return subprocess.run([sys.executable, str(HELPER), "--data-dir", str(self.data),
                               "--device-dir", str(self.device), "--", sys.executable,
                               str(self.script), "generate"], env=env, capture_output=True, text=True)

    def automatic_hook(self):
        codex = self.root / "codex"
        hook = self.root / "hook"
        hook.mkdir(exist_ok=True)
        shutil.copyfile(HELPER, hook / "aihubmix_imagegen.py")
        shutil.copyfile(HELPER.parent / "codex_imagegen_sitecustomize.py", hook / "sitecustomize.py")
        (hook / "config.json").write_text(json.dumps({"codex_home": str(codex), "data_dir": str(self.data),
                                                     "device_dir": str(self.device)}), encoding="utf-8")
        script = codex / "skills/.system/imagegen/scripts/image_gen.py"
        script.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(self.script, script)
        return hook, script

    def run_automatic(self, hook, script):
        env = os.environ.copy()
        env.update(PYTHONPATH=str(hook), OPENAI_API_KEY="stale-secret",
                   OPENAI_BASE_URL="https://stale.invalid/v1", OPENAI_PROJECT_ID="old-project")
        return subprocess.run([sys.executable, str(script), "generate"], env=env, capture_output=True, text=True)

    def test_automatic_hook_switches_pairs_without_copying_a_command(self):
        port = self.server.server_address[1]
        self.save("a", f"http://127.0.0.1:{port}/a/v1", "secret-a")
        self.save("b", f"http://127.0.0.1:{port}/b/v1", "secret-b")
        hook, script = self.automatic_hook()
        for provider, expected in (("a", ("/a/v1/images/generations", "Bearer secret-a")),
                                   ("b", ("/b/v1/images/generations", "Bearer secret-b"))):
            self.select(provider)
            result = self.run_automatic(hook, script)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(Capture.requests[-1], expected)
            self.assertNotIn("secret-a", result.stdout + result.stderr)
            self.assertNotIn("secret-b", result.stdout + result.stderr)
            self.assertNotIn("stale-secret", result.stdout + result.stderr)
        self.save("b", f"http://127.0.0.1:{port}/b/v1", "new-secret")
        self.assertEqual(self.run_automatic(hook, script).returncode, 0)
        self.assertEqual(Capture.requests[-1][1], "Bearer new-secret")
        self.save("official", "https://example.invalid/v1", "official-secret", category="official")
        self.select("official")
        before = len(Capture.requests)
        result = self.run_automatic(hook, script)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(Capture.requests), before)
        self.assertNotIn("stale-secret", result.stdout + result.stderr)

    def test_hook_is_limited_to_codex_script_and_keeps_existing_sitecustomize(self):
        hook, _ = self.automatic_hook()
        existing = self.root / "existing-python"
        existing.mkdir()
        (existing / "sitecustomize.py").write_text("import os; os.environ['ORIGINAL_SITE'] = 'kept'\n", encoding="utf-8")
        env = os.environ.copy()
        env.update(PYTHONPATH=os.pathsep.join((str(hook), str(existing))), OPENAI_API_KEY="existing-user-key")
        other = self.root / "unrelated.py"
        other.write_text("import os; print(os.environ['ORIGINAL_SITE'], os.environ['OPENAI_API_KEY'])", encoding="utf-8")
        result = subprocess.run([sys.executable, str(other)], env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "kept existing-user-key")
        other_image = self.root / "other" / "image_gen.py"
        other_image.parent.mkdir()
        other_image.write_text("import os; print(os.environ['ORIGINAL_SITE'], os.environ['OPENAI_API_KEY'])", encoding="utf-8")
        result = subprocess.run([sys.executable, str(other_image)], env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "kept existing-user-key")

    def test_broken_hook_fails_closed_instead_of_running_with_stale_environment(self):
        hook, script = self.automatic_hook()
        (hook / "config.json").unlink()
        result = self.run_automatic(hook, script)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(Capture.requests, [])
        self.assertNotIn("stale-secret", result.stdout + result.stderr)

    @unittest.skipUnless(os.environ.get("AIHUBMIX_CODEX_E2E") == "1" and shutil.which("codex"),
                         "optional local Codex CLI integration test")
    def test_codex_shell_environment_runs_the_hook_against_a_mock_api(self):
        # The fake Responses API asks a real local Codex CLI to execute the
        # skill's command. Both the chat and image APIs are loopback-only.
        hook, script = self.automatic_hook()
        stub_source = self.script
        self.save("a", f"http://127.0.0.1:{self.server.server_address[1]}/provider/v1", "pair-a-secret")
        self.select("a")
        output = []
        offered_tools = []

        class Responses(BaseHTTPRequestHandler):
            count = 0

            def log_message(self, *_args):
                pass

            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))))
                type(self).count += 1
                if type(self).count == 1:
                    offered_tools.extend(tool.get("name") or "" for tool in body.get("tools", [])
                                         if isinstance(tool, dict))
                    # Codex seeds its bundled system skills at startup. Put the
                    # mock script back after that and before the tool call.
                    shutil.copyfile(stub_source, script)
                    if os.name == "nt":
                        def ps_quote(value):
                            return "'" + str(value).replace("'", "''") + "'"
                        command = f"& {ps_quote(sys.executable)} {ps_quote(script)} generate"
                    else:
                        command = f"{shlex.quote(sys.executable)} {shlex.quote(str(script))} generate"
                    item = {"type": "function_call", "id": "fc_1", "call_id": "call_1",
                            "name": "exec_command", "arguments": json.dumps({"cmd": command})}
                else:
                    output.extend(row.get("output", "") for row in body.get("input", [])
                                  if isinstance(row, dict) and row.get("type") == "function_call_output")
                    item = {"type": "message", "id": "msg_1", "role": "assistant", "status": "completed",
                            "content": [{"type": "output_text", "text": "mock complete", "annotations": []}]}
                response = {"id": f"resp_{type(self).count}", "object": "response", "created_at": 0,
                            "status": "completed", "model": "dummy", "output": [item],
                            "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}}
                events = [{"type": "response.created", "response": dict(response, status="in_progress", output=[])},
                          {"type": "response.output_item.added", "output_index": 0, "item": item},
                          {"type": "response.output_item.done", "output_index": 0, "item": item},
                          {"type": "response.completed", "response": response}]
                body = "".join("data: " + json.dumps(event) + "\n\n" for event in events).encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Responses)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            codex = script.parents[4]
            (codex / "config.toml").write_text(
                'model = "dummy"\nmodel_provider = "mock"\n[model_providers.mock]\nname = "Mock"\n'
                f'base_url = "http://127.0.0.1:{server.server_address[1]}/v1"\n'
                'env_key = "OPENAI_API_KEY"\nwire_api = "responses"\n'
                f'[shell_environment_policy.set]\nPYTHONPATH = {json.dumps(str(hook))}\n', encoding="utf-8")
            env = os.environ.copy()
            env.update(CODEX_HOME=str(codex), HOME=str(self.root), OPENAI_API_KEY="fake-chat-secret")
            env.pop("PYTHONPATH", None)
            result = subprocess.run(["codex", "exec", "--skip-git-repo-check", "--ephemeral",
                                     "--dangerously-bypass-approvals-and-sandbox", "Run the mock command"],
                                    cwd=self.root, env=env, capture_output=True, text=True, timeout=45)
            self.assertEqual(result.returncode, 0, result.stderr[-2000:])
            self.assertIn("exec_command", offered_tools)
            print("Mock-provider image tool exposed:",
                  any("image" in name.lower() and "gen" in name.lower() for name in offered_tools))
            self.assertTrue(Capture.requests, f"tool={output!r}; stderr={result.stderr[-1500:]}")
            self.assertEqual(Capture.requests[-1], ("/provider/v1/images/generations", "Bearer pair-a-secret"))
            self.assertTrue(output)
            self.assertIn("[REDACTED]", output[0])
            self.assertNotIn("pair-a-secret", output[0])
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
            # A short-lived Codex plugin background task may still touch HOME.
            time.sleep(2)

    def test_switch_and_key_update_keep_the_pair_together_and_redact_output(self):
        port = self.server.server_address[1]
        self.save("a", f"http://127.0.0.1:{port}/a/v1", "secret-a")
        self.save("b", f"http://127.0.0.1:{port}/b/v1/", "secret-b")
        self.select("a")
        first = self.run_helper()
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(Capture.requests[-1], ("/a/v1/images/generations", "Bearer secret-a"))
        self.assertNotIn("secret-a", first.stdout + first.stderr)
        self.assertIn("[REDACTED]", first.stdout)
        self.select("b")
        second = self.run_helper()
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(Capture.requests[-1], ("/b/v1/images/generations", "Bearer secret-b"))
        self.assertNotIn("secret-a", second.stdout + second.stderr)
        self.save("b", f"http://127.0.0.1:{port}/b/v1", "secret-b-updated")
        third = self.run_helper()
        self.assertEqual(third.returncode, 0, third.stderr)
        self.assertEqual(Capture.requests[-1], ("/b/v1/images/generations", "Bearer secret-b-updated"))
        self.save("b", f"http://127.0.0.1:{port}/b/v1", "")
        before = len(Capture.requests)
        missing_key = self.run_helper()
        self.assertNotEqual(missing_key.returncode, 0)
        self.assertEqual(len(Capture.requests), before)
        self.assertNotIn("stale-secret", missing_key.stdout + missing_key.stderr)

    def test_proxy_route_and_sub2api_url(self):
        self.save("a", "https://csmy.fun:23457", "sub2api-secret")
        self.save("b", "https://other.example/v1/responses", "other-secret")
        self.select("a")
        self.assertEqual(helper.resolve_pair(self.data, self.device), ("sub2api-secret", "https://csmy.fun:23457/v1"))
        self.select("a", {"apps": {"codex": {"mode": "proxy", "proxy_route": "b"}}})
        self.assertEqual(helper.resolve_pair(self.data, self.device), ("other-secret", "https://other.example/v1"))
        with closing(sqlite3.connect(self.db)) as conn:
            conn.execute("INSERT INTO proxy_config VALUES ('codex', 1)")
            conn.commit()
        with self.assertRaises(helper.Unavailable):
            helper.resolve_pair(self.data, self.device)
        self.select("a", {"apps": {"codex": {"mode": "proxy", "proxy_route": "b", "stack": {"enabled": True}}}})
        with self.assertRaises(helper.Unavailable):
            helper.resolve_pair(self.data, self.device)

    def test_official_pending_and_invalid_config_fail_closed(self):
        self.save("official", "https://example.invalid/v1", "must-not-leak", category="official")
        self.select("official")
        result = self.run_helper()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(Capture.requests, [])
        self.assertNotIn("must-not-leak", result.stdout + result.stderr)
        self.assertNotIn("stale-secret", result.stdout + result.stderr)
        self.save("third", "https://example.invalid/v1", "secret-third")
        self.select("third", {"apps": {"codex": {"pending": {"operation": "switch"}}}})
        self.assertNotEqual(self.run_helper().returncode, 0)
        self.select("missing")
        self.assertNotEqual(self.run_helper().returncode, 0)
        self.assertEqual(Capture.requests, [])

    def test_escaped_secret_is_redacted_from_child_output(self):
        port = self.server.server_address[1]
        key = 'secret"with-quote'
        self.save("a", f"http://127.0.0.1:{port}", key)
        self.select("a")
        result = self.run_helper()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(key, result.stdout + result.stderr)
        self.assertNotIn(json.dumps(key)[1:-1], result.stdout + result.stderr)

    def test_invalid_or_ambiguous_base_urls_are_rejected(self):
        for base in ("https://user:pass@example.com/v1", "https://example.com/v1?key=secret", "https://example.com/v4", "https://example.com/custom", "file:///tmp/v1"):
            with self.subTest(base=base), self.assertRaises((helper.Unavailable, ValueError)):
                helper._base_url(base)

    def test_concurrent_selection_change_is_rejected(self):
        self.save("a", "https://a.example/v1", "secret-a")
        before = json.dumps({"currentProviderCodex": "a"}).encode()
        after = json.dumps({"currentProviderCodex": "b"}).encode()
        with mock.patch.object(helper, "_optional_bytes", side_effect=[before, b"{}", after, b"{}"] * 3):
            with self.assertRaises(helper.Unavailable):
                helper.resolve_pair(self.data, self.device)

    def test_running_child_is_stopped_when_provider_changes(self):
        port = self.server.server_address[1]
        self.save("a", f"http://127.0.0.1:{port}/a/v1", "secret-a")
        self.save("b", f"http://127.0.0.1:{port}/b/v1", "secret-b")
        self.select("a")
        marker = self.root / "ready"
        process = subprocess.Popen([sys.executable, str(HELPER), "--data-dir", str(self.data),
                                    "--device-dir", str(self.device), "--", sys.executable,
                                    str(self.script), "wait", str(marker)],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 5
            while not marker.exists() and time.monotonic() < deadline:
                time.sleep(0.05)
            self.assertTrue(marker.exists(), "child did not start")
            self.select("b")
            stdout, stderr = process.communicate(timeout=5)
            self.assertNotEqual(process.returncode, 0)
            self.assertEqual(Capture.requests, [])
            self.assertNotIn("secret-a", stdout + stderr)
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate()

    def test_non_openai_and_managed_providers_are_rejected(self):
        for meta in ({"apiFormat": "anthropic"}, {"providerType": "codex_oauth"},
                     {"authBinding": {"source": "managed_account"}}):
            self.save("a", "https://example.invalid/v1", "fake-secret", meta=meta)
            self.select("a")
            with self.subTest(meta=meta), self.assertRaises(helper.Unavailable):
                helper.resolve_pair(self.data, self.device)


if __name__ == "__main__":
    unittest.main()
