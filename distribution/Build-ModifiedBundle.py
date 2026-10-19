"""Package preview.4 x64 from verified binaries and a preview.3 dependency bundle."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import zipfile


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest().upper()


def tracked(root):
    return subprocess.check_output(['git', 'ls-files', '-z'], cwd=root).decode().split('\0')[:-1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['base', 'bar', 'wm-source', 'wm-runtime', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    version = '0.1.0-preview.4'
    folder = out / f'GlazeBar-Community-{version}-windows-x64'
    if folder.exists():
        raise RuntimeError(f'Refusing to overwrite {folder}')
    for binary in [args.bar, args.wm_runtime / 'glazewm.exe', args.wm_runtime / 'glazewm-watcher.exe']:
        data = binary.read_bytes()
        assert struct.unpack_from('<H', data, struct.unpack_from('<I', data, 60)[0] + 4)[0] == 0x8664, binary
    source = out / 'glazebar-source.zip'
    with zipfile.ZipFile(args.base / 'source/glazebar-source.zip') as old, zipfile.ZipFile(source, 'w', zipfile.ZIP_DEFLATED) as z:
        written = set()
        for item in old.infolist():
            if item.is_dir():
                continue
            name = item.filename
            replacement = repo / name
            data = replacement.read_bytes() if '/vendor/' not in name and replacement.is_file() else old.read(name)
            if name == 'native-rust/.cargo/config.toml':
                data = (repo / name).read_bytes() + b"\n[source.crates-io]\nreplace-with = 'vendored-sources'\n[source.vendored-sources]\ndirectory = 'vendor'\n"
            z.writestr(name, data)
            written.add(name)
        for name in tracked(repo):
            if name not in written and (repo / name).is_file():
                z.write(repo / name, name)
    wm_source = out / 'glazewm-modified-source.zip'
    with zipfile.ZipFile(wm_source, 'w', zipfile.ZIP_DEFLATED) as z:
        for name in sorted(set(tracked(args.wm_source) + ['.cargo/config.toml'])):
            if (args.wm_source / name).is_file():
                z.write(args.wm_source / name, name)
    shutil.copytree(args.base, folder)
    for name in ['Install.ps1', 'Uninstall.ps1', 'README.txt', 'README.ko.txt']:
        shutil.copy2(repo / 'distribution' / name, folder / name)
    for name in ['GlazeBar.exe', 'GlazeTheme.exe']:
        shutil.copy2(args.bar, folder / 'payload' / name)
    for name in ['config.yaml', 'cheatsheet.tsv']:
        shutil.copy2(repo / 'distribution/templates' / name, folder / 'payload' / name)
    (folder / 'payload/wm').mkdir()
    for name in ['glazewm.exe', 'glazewm-watcher.exe']:
        shutil.copy2(args.wm_runtime / name, folder / 'payload/wm' / name)
    for archive in [source, wm_source]:
        shutil.copy2(archive, folder / 'source' / archive.name)
    shutil.copy2(repo / 'glazewm-patches/glazewm-v3.10.1.patch', folder / 'source/glazewm-v3.10.1.patch')
    shutil.copy2(repo / 'docs/glazewm-modifications.md', folder / 'source/GLAZEWM-MODIFICATIONS.md')
    provenance = {
        'version': version, 'architecture': 'x64', 'glazewmVersion': '3.10.1-glazebar.1',
        'upstream': 'https://github.com/glzr-io/glazewm/tree/v3.10.1',
        'wmToolchain': '1.101.0-nightly (b6eeb93c9 2026-10-18)', 'barToolchain': '1.93.0',
        'barSourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo).decode().strip(),
        'modifiedWmSourceSHA256': sha(wm_source), 'wmPatchSHA256': sha(repo / 'glazewm-patches/glazewm-v3.10.1.patch'),
        'wmSHA256': sha(args.wm_runtime / 'glazewm.exe'), 'barSHA256': sha(args.bar),
    }
    (folder / 'source/BUILD-PREVIEW4.json').write_text(json.dumps(provenance, indent=2), encoding='utf-8')
    (folder / 'source/BUILD-MODIFIED-WM.txt').write_text(
        'Extract glazewm-modified-source.zip and glazewm-vendor.zip together.\n'
        'Run in an x64 MSVC developer shell (Windows SDK required):\n'
        'set VERSION_NUMBER=3.10.1-glazebar.1\n'
        'cargo +nightly build --offline --locked --release -p wm --bin glazewm\n'
        'Tested nightly: 1.101.0-nightly (b6eeb93c9 2026-10-18).\n'
        'The watcher is unchanged from upstream 3.10.1; source and dependencies are included.\n', encoding='utf-8')
    manifest = {'version': version, 'architecture': 'x64', 'glazewmVersion': '3.10.1-glazebar.1',
                'glazewmMsi': 'upstream/standalone-glazewm-v3.10.1-x64.msi',
                'files': [{'path': p.relative_to(folder).as_posix(), 'sha256': sha(p)}
                          for p in sorted(folder.rglob('*')) if p.is_file() and p.name != 'manifest.json']}
    (folder / 'manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
    with zipfile.ZipFile(str(folder) + '.zip', 'w', zipfile.ZIP_DEFLATED) as z:
        for p in sorted(folder.rglob('*')):
            if p.is_file():
                z.write(p, p.relative_to(folder).as_posix())
    print(folder)


if __name__ == '__main__':
    main()
