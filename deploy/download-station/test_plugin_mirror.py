import hashlib
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

from sync_plugin_market import tree_hash, verify_archive


class MirrorVerificationTest(unittest.TestCase):
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
