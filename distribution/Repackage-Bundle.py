"""Update a verified preview.4 bundle with GlazeBar, retaining WM and vendors."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import zipfile


def sha(data):
    return hashlib.sha256(data).hexdigest().upper()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', type=Path, required=True)
    parser.add_argument('--bar', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--version', default='0.1.0-preview.5')
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    with zipfile.ZipFile(args.base) as base:
        manifest = json.loads(base.read('manifest.json').decode('utf-8-sig'))
        arch = manifest['architecture']
        for item in manifest['files']:
            if sha(base.read(item['path'])) != item['sha256'].upper():
                raise RuntimeError('Base integrity failed: ' + item['path'])
        binary = args.bar.read_bytes()
        machine = struct.unpack_from('<H', binary, struct.unpack_from('<I', binary, 60)[0] + 4)[0]
        if machine != {'arm64': 0xAA64, 'x64': 0x8664}[arch]:
            raise RuntimeError('Wrong binary architecture')
        folder = args.output / f'GlazeBar-Community-{args.version}-windows-{arch}'
        folder.mkdir(parents=True, exist_ok=False)
        base.extractall(folder)
    tracked = subprocess.check_output(['git', 'ls-files', '-z'], cwd=repo).decode().split('\0')[:-1]
    source = folder / 'source/glazebar-source.zip'
    replacement = source.with_suffix('.new.zip')
    with zipfile.ZipFile(source) as old, zipfile.ZipFile(replacement, 'w', zipfile.ZIP_DEFLATED, strict_timestamps=False) as new:
        if old.read('native-rust/Cargo.lock').replace(b'\r\n', b'\n') != (repo / 'native-rust/Cargo.lock').read_bytes().replace(b'\r\n', b'\n'):
            raise RuntimeError('Cannot reuse vendors with a different lockfile')
        written = set()
        for entry in old.infolist():
            if entry.is_dir():
                continue
            name = entry.filename
            local = repo / name
            data = local.read_bytes() if name in tracked else old.read(name)
            if name == 'native-rust/.cargo/config.toml':
                data = local.read_bytes() + b"\n[source.crates-io]\nreplace-with = 'vendored-sources'\n[source.vendored-sources]\ndirectory = 'vendor'\n"
            new.writestr(name, data)
            written.add(name)
        for name in tracked:
            if name not in written:
                new.write(repo / name, name)
    replacement.replace(source)
    for name in ['GlazeBar.exe', 'GlazeTheme.exe']:
        (folder / 'payload' / name).write_bytes(binary)
    for name in ['Install.ps1', 'Uninstall.ps1', 'README.txt', 'README.ko.txt']:
        (folder / name).write_bytes((repo / 'distribution' / name).read_bytes())
    provenance = {
        'version': args.version, 'architecture': arch,
        'baseBundle': args.base.name, 'baseBundleSHA256': sha(args.base.read_bytes()),
        'barSourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo).decode().strip(),
        'barSHA256': sha(binary), 'glazewmVersion': manifest['glazewmVersion'],
        'unchangedComponents': ['modified GlazeWM', 'official GlazeWM MSI', 'dependency vendors', 'third-party licenses'],
    }
    (folder / 'source/BUILD-PREVIEW5.json').write_text(json.dumps(provenance, indent=2), encoding='utf-8')
    manifest['version'] = args.version
    manifest['files'] = [{'path': p.relative_to(folder).as_posix(), 'sha256': sha(p.read_bytes())}
                         for p in sorted(folder.rglob('*')) if p.is_file() and p.name != 'manifest.json']
    (folder / 'manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
    with zipfile.ZipFile(str(folder) + '.zip', 'w', zipfile.ZIP_DEFLATED, strict_timestamps=False) as archive:
        for p in sorted(folder.rglob('*')):
            if p.is_file():
                archive.write(p, p.relative_to(folder).as_posix())
    print(folder)


if __name__ == '__main__':
    main()
