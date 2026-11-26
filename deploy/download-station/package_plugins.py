"""Publish individually verified plugin archives beside a legacy mirror snapshot."""
import argparse
import hashlib
import json
import os
import re
import tarfile
import tempfile
from contextlib import ExitStack
from pathlib import Path, PurePosixPath

from sync_plugin_market import verify_archive
from native_plugin_registry import publish_registry


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def publish(source, destination):
    manifest = json.loads((source / "latest.json").read_text())
    index = manifest["index"]
    revision = index["revision"]
    if not re.fullmatch(r"[0-9a-f]{40}", revision) or manifest["archive"] != revision + ".tar.gz":
        raise ValueError("invalid snapshot revision")
    bundle = source / manifest["archive"]
    if bundle.stat().st_size != manifest["size"] or sha256(bundle) != manifest["sha256"]:
        raise ValueError("legacy archive differs from published index")
    verify_archive(bundle, index)
    plugins = {p["name"]: p for p in index["plugins"]}
    if any(not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,127}", name) for name in plugins):
        raise ValueError("invalid plugin name")
    destination.mkdir(exist_ok=True, parents=True)
    with tempfile.TemporaryDirectory(dir=destination) as temp:
        stage = Path(temp) / revision
        stage.mkdir()
        started = set()
        current = None
        with tarfile.open(bundle, "r|gz") as archive, ExitStack() as outputs:
            for member in archive:
                parts = PurePosixPath(member.name).parts
                if len(parts) < 4 or parts[1] != "plugins":
                    continue
                name = parts[2]
                if name != current:
                    if name in started or name not in plugins:
                        raise ValueError("unexpected archive ordering or plugin")
                    outputs.close()
                    current = name
                    started.add(name)
                    writer = outputs.enter_context(tarfile.open(stage / (name + ".tar.gz"), "w:gz", compresslevel=5))
                writer.addfile(member, archive.extractfile(member) if member.isfile() else None)
        for name, plugin in plugins.items():
            package = stage / (name + ".tar.gz")
            if name not in started:
                with tarfile.open(package, "w:gz"):
                    pass
            single = dict(index, plugins=[plugin])
            verify_archive(package, single)
            descriptor = dict(index=single, archive=f"packages/{revision}/{name}.tar.gz",
                              sha256=sha256(package), size=package.stat().st_size)
            (stage / (name + ".json")).write_text(json.dumps(descriptor, ensure_ascii=False, separators=(",", ":")))
        live = destination / revision
        if live.exists():
            # Keep the first published bytes for immutable URLs on retries.
            for name, plugin in plugins.items():
                p = live / (name + ".tar.gz")
                meta = json.loads((live / (name + ".json")).read_text())
                if meta["index"] != dict(index, plugins=[plugin]) or meta["sha256"] != sha256(p) or meta["size"] != p.stat().st_size:
                    raise ValueError("existing immutable package differs")
        else:
            stage.chmod(0o755)
            os.rename(stage, live)
        publish_registry(destination, index)
        pending = Path(temp) / "latest.json"
        pending.write_text(json.dumps(manifest, ensure_ascii=False, separators=(",", ":")))
        os.replace(pending, destination / "latest.json")
    print("Published", len(plugins), "verified plugin packages for", revision)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    publish(args.source, args.destination)
