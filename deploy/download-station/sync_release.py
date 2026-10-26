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
from urllib.request import Request, urlopen
from update_feed import prepare_feed

BASE = Path(os.environ.get('DATA_DIR', '/data'))
RELEASES = BASE / 'releases'
INCOMING = BASE / 'incoming'
CURRENT = BASE / 'current'
STATE = BASE / 'release-sync.json'
API_URL = 'https://api.github.com/repos/simonyuann/aihubmix-switch/releases/latest'
MAX_ASSET_BYTES = 2 * 1024**3
PLATFORM_SUFFIXES = {
    'Windows': ('.exe', '.msi', '.msix', '.msixbundle', '.zip'),
    'macOS': ('.dmg', '.pkg', '.zip', '.tar.gz'),
    'Linux': ('.appimage', '.deb', '.rpm', '.tar.gz', '.tar.xz', '.zip', '.run', '.sh'),
}


def request(url):
    return Request(url, headers={
        'Accept': 'application/vnd.github+json',
        'User-Agent': 'csmy-release-mirror/1.0',
        'X-GitHub-Api-Version': '2022-11-28',
    })


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


def release_assets(release):
    tag = str(release.get('tag_name', ''))
    if not re.fullmatch(r'v?[0-9]+\.[0-9]+\.[0-9]+(?:[-.][A-Za-z0-9.-]+)?', tag):
        raise ValueError(f'unsupported release tag: {tag!r}')
    if release.get('draft') or release.get('prerelease'):
        raise ValueError('latest release is not a stable published release')
    selected = []
    checksum = None
    platforms = set()
    for raw in release.get('assets', []):
        name = validate_name(str(raw.get('name', '')))
        size = int(raw.get('size', -1))
        url = str(raw.get('browser_download_url', ''))
        if not url.startswith('https://github.com/simonyuann/aihubmix-switch/releases/download/'):
            raise ValueError(f'unexpected asset URL for {name}')
        if not 0 < size <= MAX_ASSET_BYTES:
            raise ValueError(f'invalid asset size for {name}')
        item = {'id': int(raw.get('id', 0)), 'name': name, 'size': size, 'url': url}
        if name == 'SHA256SUMS.txt':
            checksum = item
            continue
        platform = platform_for(name.removesuffix('.sig'))
        if name == 'latest.json':
            item['platform'] = ''
            selected.append(item)
            continue
        if platform:
            item['platform'] = platform
            selected.append(item)
            platforms.add(platform)
    if checksum is None or platforms != set(PLATFORM_SUFFIXES):
        raise ValueError('release is missing checksums or a supported platform')
    if not any(item['name'] == 'latest.json' for item in selected):
        raise ValueError('release contains no updater manifest')
    return tag, selected, checksum


def download(asset, destination):
    digest = hashlib.sha256()
    total = 0
    with urlopen(request(asset['url']), timeout=60) as response, destination.open('xb') as output:
        while True:
            chunk = response.read(1024 * 1024)
            if not chunk:
                break
            total += len(chunk)
            if total > asset['size'] or total > MAX_ASSET_BYTES:
                raise ValueError(f'asset exceeded declared size: {asset["name"]}')
            output.write(chunk)
            digest.update(chunk)
        output.flush()
        os.fsync(output.fileno())
    if total != asset['size']:
        raise ValueError(f'asset size mismatch: {asset["name"]}')
    return digest.hexdigest()


def fingerprint(tag, assets, checksum):
    values = [(item['id'], item['name'], item['size']) for item in [*assets, checksum]]
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
    expected = [*assets, checksum]
    expected_paths = set()
    for item in expected:
        relative = Path(item.get('platform', '')) / item['name']
        expected_paths.add(relative.as_posix())
        path = release_dir / relative
        if not path.is_file() or path.stat().st_size != item['size']:
            return False
    actual_paths = {path.relative_to(release_dir).as_posix()
                    for path in release_dir.rglob('*') if path.is_file()}
    return actual_paths == expected_paths | {'updater.json'} and (release_dir / 'updater.json').is_file()


def atomic_json(path, value):
    temporary = path.with_name('.' + path.name + '-' + secrets.token_hex(8))
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    os.replace(temporary, path)


def remove_stale_releases(active):
    for child in RELEASES.iterdir():
        # Persistent user files never belong to release retention. Protect legacy
        # trees too, even if a migration was missed or interrupted.
        if (child.is_dir() and not child.is_symlink()
                and re.fullmatch(r'v?[0-9]+\.[0-9]+\.[0-9]+[-.A-Za-z0-9]*', child.name)
                and not (child / 'swq').exists()
                and not (child / 'swq').is_symlink()
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
    with urlopen(request(API_URL), timeout=30) as response:
        release = json.load(response)
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
        download(checksum_asset, checksum_path)
        checksums = parse_checksums(checksum_path.read_text(encoding='utf-8'))
        if any(item['name'] not in checksums for item in assets):
            raise ValueError('SHA256SUMS.txt does not cover every selected asset')
        for item in assets:
            folder = staging / item['platform']
            folder.mkdir(exist_ok=True)
            actual = download(item, folder / item['name'])
            if actual != checksums[item['name']]:
                raise ValueError(f'checksum mismatch: {item["name"]}')

        prepare_feed(staging, tag)
        os.replace(staging, destination)
        pointer = BASE / ('.current-' + secrets.token_hex(8))
        pointer.symlink_to(Path('releases') / directory_name)
        os.replace(pointer, CURRENT)
        atomic_json(STATE, {
            'tag': tag,
            'fingerprint': identity,
            'asset_count': len(assets),
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
