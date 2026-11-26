"""Refresh public catalog metadata from a clean checkout of github.com/openai/plugins."""
import argparse
import json
import re
import subprocess
from urllib.parse import urlsplit
from pathlib import Path


def catalog(repo):
    def git(*args):
        return subprocess.check_output(["git", "-C", repo, *args])

    revision = git("rev-parse", "HEAD").decode().strip()
    def read(path):
        return json.loads(git("show", revision + ":" + path))

    directory = ".agents/plugins/"
    api_names = {p["name"] for p in read(directory + "api_marketplace.json")["plugins"]}
    result = []
    for item in read(directory + "marketplace.json")["plugins"]:
        name = item["name"]
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,127}", name):
            raise ValueError(name)
        entry = dict(name=name, api=name in api_names, category=item["category"],
                     policy=item["policy"])
        source = item["source"]
        if source["source"] == "local":
            path = source["path"].removeprefix("./")
            if path != "plugins/" + name:
                raise ValueError(path)
            manifest = read(path + "/.codex-plugin/plugin.json")
            if manifest["name"] != name:
                raise ValueError(name)
            interface = manifest.get("interface", {})
            files = git("ls-tree", "-rlz", revision, "--", path).split(b"\0")
            sizes = [int(file.split(b"\t")[0].split()[3]) for file in files if file]
            entry.update(path=path, tree=git("rev-parse", revision + ":" + path).decode().strip(),
                         bytes=sum(sizes), displayName=interface.get("displayName", name),
                         version=manifest.get("version", ""), description=interface.get("shortDescription") or manifest.get("description", ""),
                         requiresAuth=bool(manifest.get("apps")), license=manifest.get("license", ""))
            mcp_file = path + "/.mcp.json"
            if entry["api"] and any(file.endswith(b"\t" + mcp_file.encode()) for file in files):
                servers = read(mcp_file).get("mcpServers", {})
                if servers and all(
                    set(config) <= {"url", "type", "oauth_resource"} and config.get("type") == "http"
                    and all(urlsplit(config[key]).scheme == "https" and urlsplit(config[key]).hostname
                            and not urlsplit(config[key]).username for key in ("url", "oauth_resource") if key in config)
                    for config in servers.values()
                ):
                    entry["remoteServers"] = servers
            if manifest.get("apps") == "./.app.json":
                apps = list(read(path + "/.app.json")["apps"].values())
                if len(apps) == 1 and re.fullmatch(r"(connector|asdk_app)_[a-z0-9]+", apps[0]["id"]):
                    entry["appId"] = apps[0]["id"]
        result.append(entry)
    if not result or len({p["name"] for p in result}) != len(result):
        raise ValueError("invalid official catalog")
    return {"revision": revision, "plugins": result}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkout", type=Path)
    args = parser.parse_args()
    output = Path(__file__).resolve().parents[2] / "src-tauri/resources/openai-plugin-catalog.json"
    output.write_text(json.dumps(catalog(args.checkout), indent=2, ensure_ascii=False) + "\n")
