"""Unit and failure-safety tests for the GitHub release synchronizer."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
import os
import time
from urllib.error import HTTPError
from update_feed import FORMATS, ORIGIN, PREFIX, update_file


class FakeResponse(io.BytesIO):
    """Minimal stand-in for the object urllib returns after following redirects."""

    def __init__(self, body=b'', url='', headers=None):
        super().__init__(body)
        self._url = url
        self.headers = headers or {}

    def geturl(self):
        return self._url


class SyncReleaseTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location(
            'sync_release', (Path(__file__).parent / 'sync_release.py'
                             if (Path(__file__).parent / 'sync_release.py').exists()
                             else Path(__file__).parents[1] / 'sync_release.py'))
        self.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.module)
        self.temp = tempfile.TemporaryDirectory()
        base = Path(self.temp.name)
        self.module.BASE = base
        self.module.RELEASES = base / 'releases'
        self.module.INCOMING = base / 'incoming'
        self.module.CURRENT = base / 'current'
        self.module.STATE = base / 'release-sync.json'

    def tearDown(self):
        self.temp.cleanup()

    def release(self, tag='v1.2.3', bad_checksum=False):
        version = tag[1:]
        content = {}
        platforms = {}
        for target, (_, suffix) in FORMATS.items():
            name = f'AIHubMix-Switch_{version}_{suffix}'
            content[name] = suffix.encode()
            content[name + '.sig'] = b'c2lnbmF0dXJl'
            platforms[target] = {
                'url': f'{ORIGIN}{PREFIX}{tag}/{name}',
                'signature': 'c2lnbmF0dXJl',
            }
        for suffix in ('macos-arm64.dmg', 'macos-x64.dmg'):
            content[f'AIHubMix-Switch_{version}_{suffix}'] = suffix.encode()
        content['latest.json'] = json.dumps({'version': version, 'platforms': platforms}).encode()
        lines = []
        for name, value in content.items():
            digest = '0' * 64 if bad_checksum and 'windows' in name else hashlib.sha256(value).hexdigest()
            lines.append(f'{digest}  {name}')
        content['SHA256SUMS.txt'] = ('\n'.join(lines) + '\n').encode()
        assets = []
        for index, (name, value) in enumerate(content.items(), 1):
            assets.append({
                'id': index + (100 if tag != 'v1.2.3' else 0),
                'name': name,
                'size': len(value),
                'browser_download_url':
                    f'https://github.com/simonyuann/aihubmix-switch/releases/download/{tag}/{name}',
            })
        base_url = f'{self.module.DOWNLOAD_PREFIX}{tag}/'
        responses = {}
        responses.update({item['browser_download_url']: content[item['name']] for item in assets})
        responses[base_url + 'SHA256SUMS.txt'] = content['SHA256SUMS.txt']
        responses[base_url + 'latest.json'] = content['latest.json']
        responses[self.module.RELEASES_LATEST_URL] = (self.tag_url(tag), b'')
        responses[self.module.RELEASES_ATOM_URL] = self.atom(tag)
        return responses

    def release_json(self, responses, tag):
        """Render the same files as an api.github.com style release payload."""
        assets = [{'id': index, 'name': url.rsplit('/', 1)[1], 'size': len(body),
                   'browser_download_url': url}
                  for index, (url, body) in enumerate(sorted(responses.items()), 1)
                  if url.startswith(self.module.DOWNLOAD_PREFIX)]
        return {'tag_name': tag, 'draft': False, 'prerelease': False,
                'published_at': '2026-10-25T00:00:00Z', 'assets': assets}

    def tag_url(self, tag):
        return f'https://github.com/simonyuann/aihubmix-switch/releases/tag/{tag}'

    def atom(self, *tags):
        entries = ''.join(
            '<entry><id>tag:github.com</id>'
            f'<link rel="alternate" type="text/html" href="{self.tag_url(tag)}"/>'
            '</entry>' for tag in tags)
        return ('<?xml version="1.0" encoding="UTF-8"?><feed>' + entries + '</feed>').encode()

    def install_responses(self, responses):
        self.calls = []

        def fake_urlopen(req, timeout=0):
            url = req.full_url
            self.calls.append(url)
            if url not in responses:
                raise HTTPError(url, 404, 'Not Found', {}, None)
            result = responses[url]
            if isinstance(result, Exception):
                raise result
            if isinstance(result, list):
                if not result:
                    raise HTTPError(url, 404, 'Not Found', {}, None)
                result = result.pop(0)
            if isinstance(result, FakeResponse):
                return result
            if isinstance(result, tuple):
                final, body = result
                return FakeResponse(body, final)
            return FakeResponse(result, url)
        self.module.urlopen = fake_urlopen

    def test_classification_and_checksum_parsing(self):
        self.assertEqual(self.module.RELEASES_LATEST_URL,
                         'https://github.com/simonyuann/aihubmix-switch/releases/latest')
        self.assertEqual(self.module.RELEASES_ATOM_URL,
                         'https://github.com/simonyuann/aihubmix-switch/releases.atom')
        self.assertEqual(self.module.checked_tag('1.2.3'), 'v1.2.3')
        self.assertEqual(self.module.checked_tag('v1.2.3-beta.1'), 'v1.2.3-beta.1')
        with self.assertRaises(ValueError):
            self.module.checked_tag('../../../etc/passwd')
        self.assertEqual(self.module.platform_for('AIHubMix-Switch_1.0_windows-x64-setup.exe'), 'Windows')
        self.assertEqual(self.module.platform_for('AIHubMix-Switch_1.0_macos-arm64.dmg'), 'macOS')
        self.assertEqual(self.module.platform_for('AIHubMix-Switch_1.0_linux-x64.AppImage'), 'Linux')
        self.assertIsNone(self.module.platform_for('../unsafe.exe'))
        digest = 'a' * 64
        self.assertEqual(self.module.parse_checksums(f'{digest}  app.exe')['app.exe'], digest)
        with self.assertRaises(ValueError):
            self.module.validate_name('../app.exe')

    def test_discovery_needs_no_api_quota(self):
        responses = self.release('v1.2.4')
        responses[self.module.API_URL] = HTTPError(
            self.module.API_URL, 403, 'rate limit exceeded', {}, None)
        self.install_responses(responses)
        self.assertTrue(self.module.sync_once())
        self.assertEqual(self.module.current_release().name.split('-')[0], 'v1.2.4')
        self.assertNotIn(self.module.API_URL, self.calls)

    def test_api_still_works_as_last_resort(self):
        responses = self.release('v1.2.3')
        del responses[self.module.RELEASES_LATEST_URL]
        del responses[self.module.RELEASES_ATOM_URL]
        responses[self.module.API_URL] = json.dumps(
            self.release_json(responses, 'v1.2.3')).encode()
        self.install_responses(responses)
        self.assertTrue(self.module.sync_once())
        self.assertEqual(self.module.current_release().name.split('-')[0], 'v1.2.3')
        self.assertIn(self.module.API_URL, self.calls)

    def test_atom_feed_covers_stale_redirect(self):
        responses = self.release('v1.2.4')
        # The /releases/latest edge cache can still point at the previous tag.
        responses[self.module.RELEASES_LATEST_URL] = (self.tag_url('v9.9.9'), b'')
        self.install_responses(responses)
        self.assertTrue(self.module.sync_once())
        self.assertEqual(self.module.current_release().name.split('-')[0], 'v1.2.4')

    def test_prerelease_tag_in_feed_is_skipped(self):
        responses = self.release('v1.2.3')
        del responses[self.module.RELEASES_LATEST_URL]
        responses[self.module.RELEASES_ATOM_URL] = self.atom('v1.3.0-beta.1', 'v1.2.3')
        self.install_responses(responses)
        self.assertTrue(self.module.sync_once())
        self.assertEqual(self.module.current_release().name.split('-')[0], 'v1.2.3')

    def test_half_uploaded_release_keeps_previous(self):
        self.install_responses(self.release())
        self.assertTrue(self.module.sync_once())
        first = self.module.current_release()
        responses = self.release('v1.2.4')
        # Assets are still uploading, so the checksum file is not published yet.
        del responses[self.module.DOWNLOAD_PREFIX + 'v1.2.4/SHA256SUMS.txt']
        responses[self.module.API_URL] = HTTPError(
            self.module.API_URL, 404, 'Not Found', {}, None)
        self.install_responses(responses)
        with self.assertRaisesRegex(RuntimeError, 'no usable release metadata'):
            self.module.sync_once()
        self.assertEqual(self.module.current_release(), first)

    def test_atomic_publish_retention_and_checksum_failure(self):
        self.install_responses(self.release())
        self.assertTrue(self.module.sync_once())
        first = self.module.current_release()
        self.assertIsNotNone(first)
        self.assertEqual(len(list(first.glob('Windows/*'))), 4)
        state = json.loads(self.module.STATE.read_text())
        self.assertEqual(state['tag'], 'v1.2.3')
        self.assertTrue(state['asset_sizes'])
        self.assertFalse(self.module.sync_once())

        self.install_responses(self.release('v1.2.4', bad_checksum=True))
        with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
            self.module.sync_once()
        self.assertEqual(self.module.current_release(), first)

        self.install_responses(self.release('v1.2.4'))
        self.assertTrue(self.module.sync_once())
        second = self.module.current_release()
        self.assertNotEqual(second, first)
        (second / 'extra.bin').write_bytes(b'not from GitHub')
        self.assertTrue(self.module.sync_once())
        repaired = self.module.current_release()
        self.assertNotEqual(repaired, second)
        self.assertFalse(self.module.sync_once())
        # Cached manifests still resolve after a switch, even without upstream.
        name = 'AIHubMix-Switch_1.2.3_windows-x64-setup.exe'
        self.assertEqual(update_file(PREFIX + 'v1.2.3/' + name, self.module.BASE).read_bytes(),
                         b'windows-x64-setup.exe')
        self.assertEqual(update_file(PREFIX + 'latest.json', self.module.BASE),
                         repaired / 'updater.json')
        for path in self.module.RELEASES.iterdir():
            if path.resolve() != repaired:
                os.utime(path, (time.time() - 8 * 86400,) * 2)
        self.assertFalse(self.module.sync_once())
        retained = [path.resolve() for path in self.module.RELEASES.iterdir() if path.is_dir()]
        self.assertEqual(retained, [repaired])

    def test_truncated_download_is_retried_before_publishing(self):
        responses = self.release('v1.2.4')
        url = (self.module.DOWNLOAD_PREFIX
               + 'v1.2.4/AIHubMix-Switch_1.2.4_windows-x64-setup.exe')
        good = responses[url]
        responses[url] = [
            FakeResponse(b'cut', url, {'Content-Length': str(len(good))}),
            good,
        ]
        self.install_responses(responses)
        self.assertTrue(self.module.sync_once())
        self.assertEqual(self.module.current_release().name.split('-')[0], 'v1.2.4')
        self.assertEqual(
            (self.module.current_release() / 'Windows'
             / 'AIHubMix-Switch_1.2.4_windows-x64-setup.exe').read_bytes(), good)

    def test_persistently_corrupted_download_never_replaces_current(self):
        self.install_responses(self.release())
        self.assertTrue(self.module.sync_once())
        first = self.module.current_release()
        responses = self.release('v1.2.4')
        url = (self.module.DOWNLOAD_PREFIX
               + 'v1.2.4/AIHubMix-Switch_1.2.4_linux-x64.deb')
        bad = b'x' * len(responses[url])
        responses[url] = [bad, bad, bad]
        self.install_responses(responses)
        with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
            self.module.sync_once()
        self.assertEqual(self.module.current_release(), first)

    def test_invalid_manifest_never_replaces_current(self):
        self.install_responses(self.release())
        self.module.sync_once()
        first = self.module.current_release()
        responses = self.release('v1.2.4')
        base_url = self.module.DOWNLOAD_PREFIX + 'v1.2.4/'
        manifest_url = base_url + 'latest.json'
        checksum_url = base_url + 'SHA256SUMS.txt'
        manifest = json.loads(responses[manifest_url])
        manifest['platforms']['windows-x86_64-nsis']['url'] = 'https://evil.example/app.exe'
        old = responses[manifest_url]
        new = json.dumps(manifest).encode()
        responses[manifest_url] = new
        responses[checksum_url] = responses[checksum_url].replace(
            hashlib.sha256(old).hexdigest().encode(), hashlib.sha256(new).hexdigest().encode())
        self.install_responses(responses)
        with self.assertRaisesRegex(ValueError, 'unexpected updater package URL'):
            self.module.sync_once()
        self.assertEqual(self.module.current_release(), first)
        for path in ('../latest.json', 'v1.2.3/../latest.json',
                     'v1.2.3/AIHubMix-Switch_1.2.3_windows-x64-setup.exe.exe'):
            with self.assertRaises((ValueError, FileNotFoundError)):
                update_file(PREFIX + path, self.module.BASE)

    def test_persistent_and_legacy_swq_survive_updates(self):
        persistent = self.module.BASE / 'persistent' / 'swq'
        persistent.mkdir(parents=True)
        protected = persistent / 'keep.msi'
        protected.write_bytes(b'user software')
        self.install_responses(self.release())
        self.module.sync_once()
        legacy = self.module.current_release()
        (legacy / 'swq').mkdir()
        (legacy / 'swq' / 'legacy.msi').write_bytes(b'legacy software')
        self.install_responses(self.release('v1.2.4'))
        self.module.sync_once()
        self.module.sync_once()
        self.assertEqual(protected.read_bytes(), b'user software')
        self.assertEqual((legacy / 'swq' / 'legacy.msi').read_bytes(), b'legacy software')


if __name__ == '__main__':
    unittest.main()
