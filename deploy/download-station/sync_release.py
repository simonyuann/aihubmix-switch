"""Atomically mirror the latest AIHubMix Switch GitHub release."""
import argparse
import hashlib
import json
import os
import re
import secrets
import shutil
import time
from datetime import datetime, timezone
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import quote, unquote
from urllib.request import Request, urlopen
from update_feed import prepare_feed, manifest_name, legacy_release, formats_for_version

BASE = Path(os.environ.get('DATA_DIR', '/data'))
RELEASES = BASE / 'releases'
INCOMING = BASE / 'incoming'
CURRENT = BASE / 'current'
STATE = BASE / 'release-sync.json'
API_URL = 'https://api.github.com/repos/simonyuann/aihubmix-switch/releases/latest'
RELEASES_LATEST_URL = 'https://github.com/simonyuann/aihubmix-switch/releases/latest'
RELEASES_ATOM_URL = 'https://github.com/simonyuann/aihubmix-switch/releases.atom'
DOWNLOAD_PREFIX = 'https://github.com/simonyuann/aihubmix-switch/releases/download/'
TAG_PATTERN = r'v?[0-9]+\.[0-9]+\.[0-9]+(?:[-.][A-Za-z0-9.-]+)?'
STABLE_TAG_PATTERN = r'v[0-9]+\.[0-9]+\.[0-9]+(?:[-.][A-Za-z0-9.-]+)?'
PRERELEASE_PATTERN = r'-[0-9A-Za-z.-]*(?:alpha|beta|rc|pre|preview|dev|nightly|canary)'
MAX_ASSET_BYTES = 2 * 1024**3
MAX_FEED_BYTES = 512 * 1024
PLATFORM_SUFFIXES = {
    'Windows': ('.exe', '.msi', '.msix', '.msixbundle', '.zip'),
    'macOS': ('.dmg', '.pkg', '.zip', '.tar.gz'),
    'Linux': ('.appimage', '.deb', '.rpm', '.tar.gz', '.tar.xz', '.zip', '.run', '.sh'),
}


def request(url):
    headers = {'User-Agent': 'csmy-release-mirror/1.1'}
    if url.startswith('https://api.github.com/'):
        headers.update({
            'Accept': 'application/vnd.github+json',
            'X-GitHub-Api-Version': '2022-11-28',
        })
        # Optional: an authenticated call raises the quota from 60/h to 5000/h.
        token = os.environ.get('GITHUB_TOKEN', '').strip()
        if token:
            headers['Authorization'] = f'Bearer {token}'
    return Request(url, headers=headers)


def platform_for(name):
    lower = name.lower()
    if not lower.startswith('aihubmix-switch_'):
        return None
    for marker, platform in (('_windows-', 'Windows'), ('_macos-', 'macOS'), ('_linux-', 'Linux')):
        if marker in lower and lower.endswith(PLATFORM_SUFFIXES[platform]):
            return platform
    return None


def validate_name(name):
    if not name or name != Path(name).name or name.startswith('.'):
        raise ValueError(f'unsafe asset name: {name!r}')
    if any(ord(char) < 32 for char in name) or not re.fullmatch(r'[A-Za-z0-9._+()-]+', name):
        raise ValueError(f'unsupported asset name: {name!r}')
    return name


def parse_checksums(value):
    result = {}
    for line in value.splitlines():
        match = re.fullmatch(r'([0-9a-fA-F]{64})\s+\*?(.+)', line.strip())
        if match:
            result[validate_name(match[2])] = match[1].lower()
    return result


def sha256_value(raw):
    value = str(raw.get('sha256', ''))
    digest = str(raw.get('digest', ''))
    if not value and digest.startswith('sha256:'):
        value = digest.removeprefix('sha256:')
    return value.lower() if re.fullmatch(r'[0-9a-fA-F]{64}', value) else None


def checked_tag(value):
    tag = str(value or '').strip()
    if not re.fullmatch(TAG_PATTERN, tag):
        raise ValueError(f'unsupported release tag: {tag!r}')
    return tag if tag.startswith('v') else 'v' + tag


def tag_from_redirect():
    """Newest stable tag from /releases/latest; costs no API quota."""
    with urlopen(request(RELEASES_LATEST_URL), timeout=30) as response:
        final = str(response.geturl())
    match = re.search(r'/releases/tag/([^/?#]+)/*$', final)
    if not match:
        raise ValueError('latest release redirect does not point at a tag')
    return checked_tag(unquote(match.group(1)))


def tag_from_atom():
    """Fallback discovery: newest stable tag listed in the release feed."""
    with urlopen(request(RELEASES_ATOM_URL), timeout=30) as response:
        raw = response.read(MAX_FEED_BYTES + 1)
    if len(raw) > MAX_FEED_BYTES:
        raise ValueError('release feed is too large')
    for href in re.findall(r'href="[^"]*?/releases/tag/([^"/]+)"',
                           raw.decode('utf-8', 'replace')):
        tag = unquote(href)
        if (re.fullmatch(STABLE_TAG_PATTERN, tag)
                and not re.search(PRERELEASE_PATTERN, tag, re.IGNORECASE)):
            return tag
    raise ValueError('release feed lists no stable release')


def release_from_files(tag):
    """Build release metadata from public release files, without the API."""
    tag = checked_tag(tag)
    base_url = DOWNLOAD_PREFIX + quote(tag, safe='') + '/'
    checksum_url = base_url + 'SHA256SUMS.txt'
    with urlopen(request(checksum_url), timeout=30) as response:
        checksum_bytes = response.read(1024 * 1024 + 1)
    if len(checksum_bytes) > 1024 * 1024:
        raise ValueError('checksum file is too large')
    checksums = parse_checksums(checksum_bytes.decode('utf-8'))
    feed_name = manifest_name(tag)
    if feed_name not in checksums:
        raise ValueError('release publishes no updater manifest')
    with urlopen(request(base_url + feed_name), timeout=30) as response:
        manifest_bytes = response.read(1024 * 1024 + 1)
    if len(manifest_bytes) > 1024 * 1024:
        raise ValueError('updater manifest is too large')
    if checksums[feed_name] != hashlib.sha256(manifest_bytes).hexdigest():
        raise ValueError('latest updater manifest failed checksum verification')
    manifest = json.loads(manifest_bytes)
    if str(manifest.get('version', '')) != tag.removeprefix('v'):
        raise ValueError('updater manifest version does not match the release tag')
    assets = []
    for name, digest in checksums.items():
        if name == feed_name or platform_for(name.removesuffix('.sig')):
            assets.append({
                'id': 0,
                'name': name,
                'size': None,
                'sha256': digest,
                'browser_download_url': base_url + quote(name, safe=''),
            })
    assets.append({
        'id': 0,
        'name': 'SHA256SUMS.txt',
        'size': None,
        'sha256': hashlib.sha256(checksum_bytes).hexdigest(),
        'browser_download_url': checksum_url,
    })
    return {
        'tag_name': tag,
        'draft': False,
        'prerelease': False,
        'published_at': None,
        'assets': assets,
    }


def latest_release():
    """Discover the newest release without depending on the rate-limited API."""
    problems = []
    for source, find_tag in (('releases/latest', tag_from_redirect),
                             ('release feed', tag_from_atom)):
        try:
            tag = find_tag()
        except (HTTPError, URLError, TimeoutError, ValueError) as exc:
            problems.append(f'{source}: {type(exc).__name__}: {exc}')
            continue
        try:
            return release_from_files(tag)
        except (HTTPError, URLError, TimeoutError, ValueError) as exc:
            problems.append(f'{tag}: {type(exc).__name__}: {exc}')
    try:
        with urlopen(request(API_URL), timeout=30) as response:
            return json.load(response)
    except (HTTPError, URLError, TimeoutError, json.JSONDecodeError) as exc:
        problems.append(f'api.github.com: {type(exc).__name__}: {exc}')
    raise RuntimeError('no usable release metadata: ' + '; '.join(problems))


def release_assets(release):
    tag = checked_tag(release.get('tag_name'))
    if release.get('draft') or release.get('prerelease'):
        raise ValueError('latest release is not a stable published release')
    feed_name = manifest_name(tag)
    selected = []
    checksum = None
    platforms = set()
    for raw in release.get('assets', []):
        name = validate_name(str(raw.get('name', '')))
        raw_size = raw.get('size')
        size = int(raw_size) if raw_size is not None else None
        sha256 = sha256_value(raw)
        url = str(raw.get('browser_download_url', ''))
        if not url.startswith(DOWNLOAD_PREFIX):
            raise ValueError(f'unexpected asset URL for {name}')
        if size is not None and not 0 < size <= MAX_ASSET_BYTES:
            raise ValueError(f'invalid asset size for {name}')
        if size is None and sha256 is None:
            raise ValueError(f'asset has neither size nor checksum: {name}')
        item = {'id': int(raw.get('id', 0)), 'name': name, 'size': size,
                'sha256': sha256, 'url': url}
        if name == 'SHA256SUMS.txt':
            checksum = item
            continue
        platform = platform_for(name.removesuffix('.sig'))
        if name == feed_name:
            item['platform'] = ''
            selected.append(item)
            continue
        if platform:
            item['platform'] = platform
            selected.append(item)
            platforms.add(platform)
    required_platforms = {platform for platform, _ in formats_for_version(tag).values()}
    if checksum is None or not required_platforms.issubset(platforms):
        raise ValueError('release is missing checksums or a supported platform')
    if not any(item['name'] == feed_name for item in selected):
        raise ValueError('release contains no updater manifest')
    return tag, selected, checksum


def download(asset, destination):
    digest = hashlib.sha256()
    total = 0
    with urlopen(request(asset['url']), timeout=60) as response, destination.open('xb') as output:
        declared = str(response.headers.get('Content-Length', '') or '')
        while True:
            chunk = response.read(1024 * 1024)
            if not chunk:
                break
            total += len(chunk)
            if total > MAX_ASSET_BYTES or (asset['size'] is not None and total > asset['size']):
                raise ValueError(f'asset exceeded declared size: {asset["name"]}')
            output.write(chunk)
            digest.update(chunk)
        output.flush()
        os.fsync(output.fileno())
    # A proxy or a dropped connection can end the stream early without raising.
    if declared.isdigit() and int(declared) != total:
        raise ValueError(f'truncated download: {asset["name"]} ({total} of {declared} bytes)')
    if asset['size'] is not None and total != asset['size']:
        raise ValueError(f'asset size mismatch: {asset["name"]}')
    return digest.hexdigest()


def download_verified(asset, destination, expected=None, attempts=3):
    """Re-fetch once or twice instead of waiting a whole interval for a flaky stream."""
    error = None
    for attempt in range(attempts):
        if destination.exists():
            destination.unlink()
        try:
            error = None
            actual = download(asset, destination)
            if expected is None or actual == expected:
                return
            error = ValueError(f'checksum mismatch: {asset["name"]}')
        except (HTTPError, URLError, TimeoutError, ValueError) as exc:
            error = exc
        if attempt + 1 < attempts:
            print(f'retrying {asset["name"]} ({attempt + 1}/{attempts}): {error}', flush=True)
    raise error if isinstance(error, ValueError) else ValueError(str(error))


def fingerprint(tag, assets, checksum):
    values = []
    for item in [*assets, checksum]:
        identity = item['sha256'] or f'{item["id"]}:{item["size"]}'
        values.append((item['name'], identity))
    payload = json.dumps([tag, values], separators=(',', ':'), ensure_ascii=True).encode()
    return hashlib.sha256(payload).hexdigest()


def current_release():
    try:
        target = CURRENT.resolve(strict=True)
        root = RELEASES.resolve(strict=True)
    except OSError:
        return None
    return target if target.is_dir() and target.is_relative_to(root) else None


def published_is_current(release_dir, assets, checksum, expected_fingerprint):
    try:
        state = json.loads(STATE.read_text())
    except (OSError, ValueError):
        return False
    if current_release() != release_dir.resolve() or state.get('fingerprint') != expected_fingerprint:
        return False
    stored_sizes = state.get('asset_sizes', {})
    expected = [*assets, checksum]
    expected_paths = set()
    for item in expected:
        relative = Path(item.get('platform', '')) / item['name']
        expected_paths.add(relative.as_posix())
        path = release_dir / relative
        expected_size = item['size']
        if expected_size is None:
            expected_size = stored_sizes.get(relative.as_posix())
        if (not isinstance(expected_size, int) or expected_size <= 0
                or not path.is_file() or path.stat().st_size != expected_size):
            return False
    actual_paths = {path.relative_to(release_dir).as_posix()
                    for path in release_dir.rglob('*') if path.is_file()}
    return actual_paths == expected_paths | {'updater.json'} and (release_dir / 'updater.json').is_file()


def atomic_json(path, value):
    temporary = path.with_name('.' + path.name + '-' + secrets.token_hex(8))
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    os.replace(temporary, path)


def remove_stale_releases(active):
    compatible = legacy_release(RELEASES)
    for child in RELEASES.iterdir():
        # Persistent user files never belong to release retention. Protect legacy
        # trees too, even if a migration was missed or interrupted.
        if (child.is_dir() and not child.is_symlink()
                and re.fullmatch(r'v?[0-9]+\.[0-9]+\.[0-9]+[-.A-Za-z0-9]*', child.name)
                and not (child / 'swq').exists()
                and not (child / 'swq').is_symlink()
                and child != compatible
                and child.resolve() != active.resolve()
                and time.time() - child.stat().st_mtime > 7 * 86400):
            shutil.rmtree(child)


def sync_once():
    BASE.mkdir(parents=True, exist_ok=True)
    RELEASES.mkdir(parents=True, exist_ok=True)
    INCOMING.mkdir(parents=True, exist_ok=True)
    for abandoned in INCOMING.glob('sync-*'):
        if abandoned.is_dir() and not abandoned.is_symlink():
            shutil.rmtree(abandoned)
    release = latest_release()
    tag, assets, checksum_asset = release_assets(release)
    identity = fingerprint(tag, assets, checksum_asset)
    directory_name = f'{tag}-{identity[:12]}'
    destination = RELEASES / directory_name
    active = current_release()
    if active and published_is_current(active, assets, checksum_asset, identity):
        remove_stale_releases(active)
        print(f'{tag} is already current ({len(assets)} assets)', flush=True)
        return False
    if destination.exists():
        directory_name += '-' + secrets.token_hex(4)
        destination = RELEASES / directory_name

    staging = INCOMING / f'sync-{directory_name}-{secrets.token_hex(8)}'
    staging.mkdir()
    try:
        checksum_path = staging / checksum_asset['name']
        download_verified(
            checksum_asset, checksum_path, checksum_asset['sha256']
        )
        checksums = parse_checksums(checksum_path.read_text(encoding='utf-8'))
        if any(item['name'] not in checksums for item in assets):
            raise ValueError('SHA256SUMS.txt does not cover every selected asset')
        for item in assets:
            folder = staging / item['platform']
            folder.mkdir(exist_ok=True)
            download_verified(item, folder / item['name'], checksums[item['name']])

        prepare_feed(staging, tag)
        os.replace(staging, destination)
        pointer = BASE / ('.current-' + secrets.token_hex(8))
        pointer.symlink_to(Path('releases') / directory_name)
        os.replace(pointer, CURRENT)
        asset_sizes = {
            path.relative_to(destination).as_posix(): path.stat().st_size
            for path in destination.rglob('*') if path.is_file() and path.name != 'updater.json'
        }
        atomic_json(STATE, {
            'tag': tag,
            'fingerprint': identity,
            'asset_count': len(assets),
            'asset_sizes': asset_sizes,
            'published_at': release.get('published_at'),
            'synced_at': datetime.now(timezone.utc).isoformat(),
        })
        print(f'published {tag} ({len(assets)} assets)', flush=True)
        return True
    finally:
        if staging.exists():
            shutil.rmtree(staging)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--interval', type=int, default=0)
    args = parser.parse_args()
    while True:
        try:
            sync_once()
        except Exception as exc:
            print(f'sync failed: {type(exc).__name__}: {exc}', flush=True)
        if args.interval <= 0:
            break
        time.sleep(max(args.interval, 60))


if __name__ == '__main__':
    main()
