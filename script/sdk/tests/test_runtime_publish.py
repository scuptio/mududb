"""Runtime publication rejects stale or modified artifacts before contacting GitHub."""
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from publish_runtime import publish_runtime

class RuntimePublishTests(unittest.TestCase):
    def archive(self, root, **changes):
        metadata = dict(source_commit='a' * 40, abi=1, dirty=False,
                        target='x86_64-unknown-linux-gnu', version='0.1.0')
        metadata.update(changes)
        data = json.dumps(metadata).encode()
        archive = root / 'mudu-adapter-test.tar.gz'
        with tarfile.open(archive, 'w:gz') as tar:
            member = tarfile.TarInfo('mudu-adapter/runtime.json')
            member.size = len(data)
            tar.addfile(member, io.BytesIO(data))
        archive.with_suffix('.gz.sha256').write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + archive.name + '\n')
        return archive

    def test_dirty_and_wrong_commit_never_contact_github(self):
        for changes in ({'dirty': True}, {'source_commit': 'b' * 40}, {'abi': 2}):
            with self.subTest(changes=changes), tempfile.TemporaryDirectory() as tmp:
                self.archive(Path(tmp), **changes)
                with patch('publish_runtime.GitHub') as api:
                    with self.assertRaises(ValueError):
                        publish_runtime(Path(tmp), 'mududb/mududb', 'a' * 40, 'test-token')
                    api.assert_not_called()

    def test_tampered_archive_never_contacts_github(self):
        with tempfile.TemporaryDirectory() as tmp:
            archive = self.archive(Path(tmp))
            archive.write_bytes(archive.read_bytes() + b'changed')
            with patch('publish_runtime.GitHub') as api:
                with self.assertRaisesRegex(ValueError, 'checksum'):
                    publish_runtime(Path(tmp), 'mududb/mududb', 'a' * 40, 'test-token')
                api.assert_not_called()

    def test_release_tag_is_pinned_to_source_commit(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.archive(Path(tmp))
            with patch('publish_runtime.GitHub') as api:
                api.return_value.request.return_value = None
                publish_runtime(Path(tmp), 'mududb/mududb', 'a' * 40, 'test-token')
                api.return_value.request.assert_any_call('POST', '/repos/mududb/mududb/git/refs',
                    {'ref': 'refs/tags/sdk-runtime-v0.1.0-aaaaaaaaaaaa', 'sha': 'a' * 40})
                self.assertEqual(api.return_value.release.call_count, 1)

if __name__ == '__main__':
    unittest.main()
