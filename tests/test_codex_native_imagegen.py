"""Exercise the real Codex CLI against local fake Responses and Images APIs.

This never calls a paid service and never uses the user's Codex home or keys.
Run explicitly with: python3 -m unittest discover -s tests -p test_codex_native_imagegen.py
"""
import base64
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest

PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZcAAAAASUVORK5CYII="


class FakeApi(http.server.ThreadingHTTPServer):
    def __init__(self, image_tool):
        super().__init__(("127.0.0.1", 0), FakeHandler)
        self.image_tool = image_tool
        self.requests = []
        self.thread = threading.Thread(target=self.serve_forever, daemon=True)
        self.thread.start()

    def close(self):
        self.shutdown()
        self.server_close()
        self.thread.join()


class FakeHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        data = self.rfile.read(int(self.headers.get("content-length", 0)))
        headers = {k.lower(): v for k, v in self.headers.items()}
        body = json.loads(data)
        self.server.requests.append((self.path, headers, body))
        if self.path.startswith("/v1/images/generations"):
            content = json.dumps({"created": 1, "data": [{"b64_json": PNG}]}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
        elif self.path.startswith("/v1/responses"):
            count = sum(path.startswith("/v1/responses") for path, _, _ in self.server.requests)
            if self.server.image_tool and count == 1:
                item = {"type": "function_call", "call_id": "call_fixture", "namespace": "image_gen",
                        "name": "imagegen", "arguments": json.dumps({"prompt": "A blue square"})}
            else:
                item = {"id": "msg_fixture", "type": "message", "role": "assistant", "status": "completed",
                        "content": [{"type": "output_text", "text": "ok", "annotations": []}]}
            events = [{"type": "response.created", "response": {"id": "resp_fixture", "status": "in_progress", "output": []}},
                      {"type": "response.output_item.done", "item": item},
                      {"type": "response.completed", "response": {"id": "resp_fixture", "status": "completed", "output": [],
                       "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}}}]
            content = "".join("event: " + e["type"] + "\ndata: " + json.dumps(e) + "\n\n" for e in events).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
        else:
            self.send_response(404)
            self.end_headers()
            return
        self.send_header("Content-Length", str(len(content)))
        self.end_headers()
        self.wfile.write(content)


def fake_free_auth():
    def encode(value):
        return base64.urlsafe_b64encode(json.dumps(value).encode()).decode().rstrip("=")
    token = encode({"alg": "none", "typ": "JWT"}) + "." + encode({
        "email": "fixture@example.com", "https://api.openai.com/auth": {
            "chatgpt_account_id": "fixture", "chatgpt_plan_type": "free"}}) + ".c2ln"
    return {"auth_mode": "chatgpt", "OPENAI_API_KEY": None, "tokens": {
        "id_token": token, "access_token": token, "refresh_token": "fixture", "account_id": "fixture"},
        "last_refresh": "2099-01-01T00:00:00Z"}


@unittest.skipUnless(shutil.which("codex"), "Codex CLI is not installed")
class NativeImageGenerationTest(unittest.TestCase):
    def test_native_call_and_provider_switch_keep_credentials_scoped(self):
        first, second = FakeApi(True), FakeApi(False)
        try:
            with tempfile.TemporaryDirectory() as directory:
                home = Path(directory)
                auth = json.dumps(fake_free_auth())
                (home / "auth.json").write_text(auth)
                env = {k: v for k, v in os.environ.items() if k.upper() not in {
                    "OPENAI_API_KEY", "OPENAI_BASE_URL", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"}}
                env.update({"CODEX_HOME": directory, "NO_PROXY": "*"})

                def run(server, key, marker):
                    header = 'http_headers = { "x-openai-actor-authorization" = "local-image-extension" }\n' if marker else ""
                    store = 'cli_auth_credentials_store = "ephemeral"\n' if marker else ""
                    (home / "config.toml").write_text(store + 'model_provider = "relay"\nmodel = "gpt-5.5"\n'
                        '[model_providers.relay]\nname = "NewAPI"\nbase_url = "http://127.0.0.1:' + str(server.server_port) +
                        '/v1"\nwire_api = "responses"\nrequires_openai_auth = false\nexperimental_bearer_token = "' + key + '"\n' + header)
                    return subprocess.run(["codex", "exec", "--ephemeral", "--skip-git-repo-check", "--json", "Draw a square"],
                        cwd=directory, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=90)

                result = run(first, "fixture-first", True)
                self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace")[-1000:])
                responses = [r for r in first.requests if r[0].startswith("/v1/responses")]
                self.assertTrue(any(t.get("name") == "image_gen" for t in responses[0][2]["tools"]))
                image = [r for r in first.requests if r[0].startswith("/v1/images/generations")]
                self.assertEqual(len(image), 1)
                self.assertEqual(image[0][1].get("authorization"), "Bearer fixture-first")
                self.assertEqual(image[0][1].get("x-openai-actor-authorization"), "local-image-extension")
                self.assertEqual(image[0][2]["prompt"], "A blue square")
                self.assertEqual((home / "auth.json").read_text(), auth)

                first_count = len(first.requests)
                result = run(second, "fixture-second", False)
                self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace")[-1000:])
                self.assertEqual(len(first.requests), first_count)
                request = next(r for r in second.requests if r[0].startswith("/v1/responses"))
                self.assertEqual(request[1].get("authorization"), "Bearer fixture-second")
                self.assertNotIn("x-openai-actor-authorization", request[1])
                self.assertFalse(any(t.get("name") == "image_gen" for t in request[2]["tools"]))
        finally:
            first.close()
            second.close()


if __name__ == "__main__":
    unittest.main()
