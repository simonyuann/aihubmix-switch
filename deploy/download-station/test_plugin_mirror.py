import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

from native_plugin_registry import publish_registry, integrity
from sync_plugin_market import tree_hash, verify_archive


class MirrorVerificationTest(unittest.TestCase):
    def test_native_packages_preserve_authored_files_and_retry_immutably(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            revision = "f" * 40
            (root / revision).mkdir()
            contents = b'{"name":"authored-name","private":true}'
            digest = hashlib.sha1(f"blob {len(contents)}\0".encode() + contents).digest()
            plugin = {"name": "demo", "bytes": len(contents),
                      "tree": tree_hash([("package.json", "100644", digest)]).hex()}
            index = {"revision": revision, "plugins": [plugin]}
            with tarfile.open(root / revision / "demo.tar.gz", "w:gz") as out:
                item = tarfile.TarInfo("snapshot/plugins/demo/package.json")
                item.size = len(contents)
                out.addfile(item, io.BytesIO(contents))
            publish_registry(root, index)
            directory = root / "npm-v1" / revision
            metadata = json.loads((directory / "aihubmix-plugin-demo").read_text())
            version = metadata["dist-tags"]["latest"]
            target = directory / "demo.tgz"
            original = target.read_bytes()
            self.assertEqual(metadata["versions"][version]["dist"]["integrity"], integrity(target))
            with tarfile.open(target) as archive:
                self.assertEqual(archive.extractfile("package/package.json").read(), contents)
            publish_registry(root, index)
            self.assertEqual(target.read_bytes(), original)
            target.write_bytes(b"damaged")
            with self.assertRaisesRegex(ValueError, "differs"):
                publish_registry(root, index)

    def test_rejects_changed_content_and_size(self):
        data = b"hello"
        blob = hashlib.sha1(b"blob 5\0" + data).digest()
        tree = tree_hash([("file.txt", "100644", blob)]).hex()
        index = {"plugins": [{"name": "demo", "bytes": 5, "tree": tree}]}
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "bundle.tgz"
            with tarfile.open(archive, "w:gz") as out:
                info = tarfile.TarInfo("revision/plugins/demo/file.txt")
                info.size = len(data)
                info.mode = 0o644
                out.addfile(info, io.BytesIO(data))
            verify_archive(archive, index)
            index["plugins"][0]["tree"] = "0" * 40
            with self.assertRaisesRegex(ValueError, "tree mismatch"):
                verify_archive(archive, index)
            index["plugins"][0]["bytes"] = 4
            with self.assertRaisesRegex(ValueError, "size mismatch"):
                verify_archive(archive, index)


if __name__ == "__main__":
    unittest.main()
