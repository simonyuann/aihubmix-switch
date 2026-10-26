"""Validated updater feed and immutable package routes for the CSMY mirror."""
import json
import re

ORIGIN = 'https://dl.csmy.fun:23457'
PREFIX = '/updates/aihubmix-switch/'
FORMATS = {
    'darwin-aarch64-app': ('macOS', 'macos-arm64.app.tar.gz'),
    'darwin-x86_64-app': ('macOS', 'macos-x64.app.tar.gz'),
    'windows-x86_64-nsis': ('Windows', 'windows-x64-setup.exe'),
    'windows-x86_64-portable': ('Windows', 'windows-x64-portable.exe'),
    'linux-x86_64-appimage': ('Linux', 'linux-x64.AppImage'),
    'linux-x86_64-deb': ('Linux', 'linux-x64.deb'),
}


def manifest_name(tag):
    version = tag.removeprefix('v')
    if not re.fullmatch(r'\d+\.\d+\.\d+', version):
        raise ValueError('feed requires a stable semantic version')
    return 'latest-v2.json' if tuple(map(int, version.split('.'))) >= (1, 1, 0) else 'latest.json'


def formats_for_version(tag):
    # CC rebuild releases currently ship macOS and Windows only.
    return {target: value for target, value in FORMATS.items()
            if manifest_name(tag) == 'latest.json' or not target.startswith('linux-')}


def legacy_release(releases):
    """Keep only an already validated 1.0.x snapshot on the old macOS-11 feed.

    updater.json is written by prepare_feed only after the complete release
    passes checksum, URL, format and signature-file consistency checks.
    Never bootstrap this feed from an unchecked latest.json or fall back to v2.
    """
    candidates = []
    for directory in releases.iterdir():
        if directory.is_symlink() or not directory.is_dir():
            continue
        try:
            feed = directory / 'updater.json'
            if feed.is_symlink():
                continue
            manifest = json.loads(feed.read_text())
            version = manifest.get('version', '')
            if not re.fullmatch(r'1\.0\.\d+', version):
                continue
            if set(manifest.get('platforms', {})) != set(FORMATS):
                continue
            # An incomplete snapshot must not resurrect a broken legacy feed.
            valid = all((directory / platform / (f'AIHubMix-Switch_{version}_{suffix}' + extra)).is_file()
                        and not (directory / platform / (f'AIHubMix-Switch_{version}_{suffix}' + extra)).is_symlink()
                        for platform, suffix in FORMATS.values() for extra in ('', '.sig'))
            if valid:
                candidates.append((tuple(map(int, version.split('.'))), directory.stat().st_mtime_ns, directory.name, directory))
        except (OSError, ValueError, TypeError):
            continue
    return max(candidates, default=(None, None, None, None))[3]


def prepare_feed(directory, tag):
    """Called only after every upstream file passed its SHA256 check."""
    manifest = json.loads((directory / manifest_name(tag)).read_text())
    version = tag.removeprefix('v')
    formats = formats_for_version(tag)
    if manifest.get('version') != version or set(manifest.get('platforms', {})) != set(formats):
        raise ValueError('updater manifest version or formats do not match release')
    for target, (platform, suffix) in formats.items():
        name = f'AIHubMix-Switch_{version}_{suffix}'
        asset = manifest['platforms'][target]
        mirror = f'{ORIGIN}{PREFIX}v{version}/{name}'
        upstream = f'https://github.com/simonyuann/aihubmix-switch/releases/download/{tag}/{name}'
        if asset.get('url') not in (mirror, upstream):
            raise ValueError('unexpected updater package URL')
        package = directory / platform / name
        signature = (directory / platform / (name + '.sig')).read_text().strip()
        if (not package.is_file() or not signature or asset.get('signature') != signature
                or not re.fullmatch(r'[A-Za-z0-9+/=]+', signature)):
            raise ValueError('missing package or inconsistent updater signature')
        asset['url'] = mirror
    (directory / 'updater.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')


def update_file(path, base):
    """Resolve only known feed/package URLs; callers return 404 on missing paths."""
    if not path.startswith(PREFIX):
        return None
    relative = path[len(PREFIX):]
    releases = (base / 'releases').resolve(strict=True)
    current = (base / 'current').resolve(strict=True)
    if not current.is_dir() or not current.is_relative_to(releases):
        raise ValueError('invalid current release')
    if relative == 'latest.json':
        legacy = legacy_release(releases)
        if legacy is None:
            raise FileNotFoundError('no verified compatible 1.0.x release')
        return legacy / 'updater.json'
    if relative == 'latest-v2.json':
        feed = current / 'updater.json'
        manifest = json.loads(feed.read_text())
        if manifest_name(str(manifest.get('version', ''))) != 'latest-v2.json':
            raise FileNotFoundError('no verified CC rebuild release')
        return feed
    match = re.fullmatch(r'v(\d+\.\d+\.\d+)/(AIHubMix-Switch_[A-Za-z0-9._-]+)', relative)
    if not match:
        raise ValueError('invalid updater path')
    version, name = match.groups()
    platform = next((folder for folder, suffix in FORMATS.values()
                     if name == f'AIHubMix-Switch_{version}_{suffix}'), None)
    if platform is None:
        raise ValueError('unknown updater package')
    # Public listings still show only current. Retained versions keep downloads
    # valid when a new release is mirrored between checking and downloading.
    for directory in [current, *sorted(releases.glob(f'v{version}-*'), reverse=True)]:
        if directory.is_symlink() or not directory.is_dir():
            continue
        feed = directory / 'updater.json'
        if not feed.is_file() or json.loads(feed.read_text()).get('version') != version:
            continue
        package = directory / platform / name
        if package.is_file() and not package.is_symlink():
            return package
    raise FileNotFoundError('updater package not found')
