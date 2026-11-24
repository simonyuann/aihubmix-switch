"""Generate a read-only npm registry for the desktop plugin installer."""
import base64
import copy
import hashlib
import io
import json
import os
import re
from pathlib import Path, PurePosixPath
import tarfile
import tempfile

from sync_plugin_market import verify_archive

REGISTRY_BASE = "https://dl.csmy.fun/files/manual/aihub-plugin-market/packages/npm-v1/"


def integrity(path):
    with path.open("rb") as stream:
        h = hashlib.sha512()
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
        digest = h.digest()
    return "sha512-" + base64.b64encode(digest).decode("ascii")


def pack(source, target, plugin, identity):
    # The per-plugin archive has already been verified against its authored Git tree.
    has_package_json = False
    with tarfile.open(source, "r:gz") as src, tarfile.open(target, "w:gz", compresslevel=5) as dst:
        for member in src:
            parts = PurePosixPath(member.name).parts
            if len(parts) < 4 or parts[1:3] != ("plugins", plugin) or not member.isfile():
                continue
            relative = parts[3:]
            if ".." in relative or any("\\" in part for part in relative):
                raise ValueError("invalid plugin package path")
            has_package_json |= relative == ("package.json",)
            output = copy.copy(member)
            output.name = "package/" + "/".join(relative)
            output.pax_headers = {}
            dst.addfile(output, src.extractfile(member))
        if not has_package_json:
            data = json.dumps(identity).encode()
            entry = tarfile.TarInfo("package/package.json")
            entry.size = len(data)
            dst.addfile(entry, io.BytesIO(data))


def publish_registry(packages, index, registry_base=REGISTRY_BASE):
    revision = index["revision"]
    if not re.fullmatch(r"[0-9a-f]{40}", revision) or any(
        not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,127}", p["name"]) for p in index["plugins"]
    ):
        raise ValueError("invalid registry index")
    parent = packages / "npm-v1"
    parent.mkdir(exist_ok=True)
    live = parent / revision
    version = "0.0.0-r" + revision
    with tempfile.TemporaryDirectory(dir=parent) as temp:
        stage = Path(temp) / revision
        stage.mkdir(mode=0o755)
        for plugin in index["plugins"]:
            name = plugin["name"]
            package = "aihubmix-plugin-" + name
            identity = {"name": package, "version": version}
            tarball = name + ".tgz"
            url = registry_base + revision + "/" + tarball
            if live.exists():
                doc = json.loads((live / package).read_text())
                expected = {"tarball": url, "integrity": integrity(live / tarball)}
                if doc["versions"][version] != dict(identity, dist=expected):
                    raise ValueError("existing native plugin package differs: " + name)
                continue
            target = stage / tarball
            source = packages / revision / (name + ".tar.gz")
            verify_archive(source, dict(index, plugins=[plugin]))
            pack(source, target, name, identity)
            dist = {"tarball": url, "integrity": integrity(target)}
            doc = {"name": package, "dist-tags": {"latest": version},
                   "versions": {version: dict(identity, dist=dist)}}
            (stage / package).write_text(json.dumps(doc, separators=(",", ":")), encoding="utf-8")
        if not live.exists():
            os.rename(stage, live)

if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Prepare native packages for the current download-station snapshot")
    parser.add_argument("packages", type=Path)
    args = parser.parse_args()
    snapshot = json.loads((args.packages / "latest.json").read_text())
    publish_registry(args.packages, snapshot["index"])
