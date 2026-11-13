#!/usr/bin/env python3
"""Build an atomic, integrity-checked mirror of the community plugin catalog."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import tarfile
import tempfile
import urllib.request

REPO = "BigPizzaV3/CodexPlusPlusPluginCache"
INDEX_URL = f"https://raw.githubusercontent.com/{REPO}/main/plugin-index.json"
INDEX_API = f"https://api.github.com/repos/{REPO}/contents/plugin-index.json"
ARCHIVE_URL = f"https://codeload.github.com/{REPO}/tar.gz/"
MAX_ARCHIVE = 1024 * 1024 * 1024


def get(url, timeout=120):
    headers = {"User-Agent": "AIHub-plugin-mirror/1", "Accept": "application/vnd.github.raw+json"}
    return urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=timeout)


def tree_hash(entries):
    payload = b"".join(mode.encode() + b" " + name.encode() + b"\0" + digest for name, mode, digest in sorted(entries, key=lambda x: (x[0] + ("/" if x[1] == "40000" else "")).encode()))
    return hashlib.sha1(b"tree " + str(len(payload)).encode() + b"\0" + payload).digest()


def verify_archive(path, index):
    plugins = {p["name"]: p for p in index["plugins"]}
    if len(plugins) != len(index["plugins"]):
        raise ValueError("duplicate plugin name")
    trees = {name: {} for name in plugins}
    sizes = {name: 0 for name in plugins}
    seen = set()
    with tarfile.open(path, "r:gz") as archive:
        for item in archive:
            parts = PurePosixPath(item.name).parts
            if len(parts) < 4 or parts[1] != "plugins":
                continue
            name, rel = parts[2], parts[3:]
            if name not in plugins or any(x in ("", ".", "..") or "\\" in x for x in rel):
                raise ValueError("unexpected plugin archive path")
            key = (name, "/".join(rel).casefold())
            if key in seen:
                raise ValueError("duplicate plugin path")
            seen.add(key)
            if item.isdir():
                continue
            if not item.isfile() or item.size > 512 * 1024 * 1024:
                raise ValueError("unsupported plugin entry")
            digest = hashlib.sha1(b"blob " + str(item.size).encode() + b"\0")
            src = archive.extractfile(item)
            for block in iter(lambda: src.read(1024 * 1024), b""):
                digest.update(block)
            sizes[name] += item.size
            trees[name][rel] = ("100755" if item.mode & 0o111 else "100644", digest.digest())
    for name, plugin in plugins.items():
        if sizes[name] != plugin["bytes"]:
            raise ValueError("plugin size mismatch: " + name)
        files = trees[name]
        dirs = {()}
        for path_parts in files:
            dirs.update(path_parts[:i] for i in range(1, len(path_parts)))
        computed = {}
        for directory in sorted(dirs, key=len, reverse=True):
            children = [(p[-1], mode, digest) for p, (mode, digest) in files.items() if p[:-1] == directory]
            children += [(p[-1], "40000", digest) for p, digest in computed.items() if p[:-1] == directory]
            computed[directory] = tree_hash(children)
        if computed[()].hex() != plugin["tree"]:
            raise ValueError("plugin tree mismatch: " + name)


def sync(root):
    root.mkdir(parents=True, exist_ok=True)
    try:
        with get(INDEX_URL, timeout=10) as r:
            index = json.load(r)
    except (OSError, ValueError):
        with get(INDEX_API, timeout=30) as r:
            index = json.load(r)
    revision = index.get("revision", "")
    if index.get("schemaVersion") != 1 or index.get("repository") != REPO or not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("invalid upstream index")
    try:
        current = json.loads((root / "latest.json").read_text(encoding="utf-8"))
        bundle = root / (revision + ".tar.gz")
        if current.get("index") == index and current.get("archive") == bundle.name and bundle.is_file() and bundle.stat().st_size == current.get("size"):
            digest = hashlib.sha256()
            with bundle.open("rb") as src:
                for chunk in iter(lambda: src.read(1024 * 1024), b""):
                    digest.update(chunk)
            if digest.hexdigest() == current.get("sha256"):
                print("already current", revision)
                return
    except (OSError, ValueError, TypeError):
        pass
    with tempfile.TemporaryDirectory(prefix=".plugin-sync-", dir=root) as tmp:
        archive = Path(tmp) / "bundle.tar.gz"
        digest = hashlib.sha256()
        total = 0
        with get(ARCHIVE_URL + revision) as src, archive.open("wb") as out:
            while chunk := src.read(1024 * 1024):
                total += len(chunk)
                if total > MAX_ARCHIVE:
                    raise ValueError("plugin archive exceeds size limit")
                digest.update(chunk)
                out.write(chunk)
        verify_archive(archive, index)
        bundle_name = revision + ".tar.gz"
        os.replace(archive, root / bundle_name)
        metadata = {"index": index, "archive": bundle_name, "sha256": digest.hexdigest(), "size": total}
        pending = Path(tmp) / "latest.json"
        pending.write_text(json.dumps(metadata, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
        os.replace(pending, root / "latest.json")
        print("published", revision, "plugins", len(index["plugins"]), "bytes", total)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    directory = parser.parse_args().directory
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / ".sync.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print("plugin mirror sync already running")
        else:
            sync(directory)
