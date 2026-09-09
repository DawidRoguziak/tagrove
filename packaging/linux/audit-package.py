#!/usr/bin/env python3
"""Audit exported package contents, binary strings and corresponding source."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('privacy', ROOT / 'scripts/privacy-check.py')
privacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(privacy)


def audit(extracted, export, temporary):
    scan = temporary / 'scan'
    scan.mkdir()
    names = {}
    failures = []
    reviewed_paths = json.loads((ROOT / 'packaging/linux/reviewed-library-paths.json').read_text())
    allowed_path_files = {(row['path'], row['binaryDigest']) for row in reviewed_paths}

    def record(name, data, source_name=None, reviewed_paths=False):
        if not reviewed_paths and privacy.private_paths(data, [source_name] if source_name else ()):
            failures.append(f'Private home path: {name}')
        key = str(len(names))
        names[key] = name
        (scan / key).write_bytes(data)

    for prefix, directory in [('package', extracted), ('export', export)]:
        for path in sorted(directory.rglob('*')):
            relative = path.relative_to(directory).as_posix()
            if privacy.private_paths(os.fsencode(relative)):
                failures.append(f'Private path in filename: {prefix}/{relative}')
            if privacy.forbidden(relative) and not (prefix == 'export' and relative == 'application-source.tar.gz'):
                failures.append(f'Local application/configuration file: {prefix}/{relative}')
            if path.is_symlink():
                record(f'{prefix}/{relative}', os.fsencode(os.readlink(path)))
                continue
            if not path.is_file():
                continue
            if prefix == 'export' and relative == 'application-source.tar.gz':
                continue
            data = path.read_bytes()
            reviewed_paths = (f'{prefix}/{relative}', hashlib.sha256(data).hexdigest()) in allowed_path_files
            if data.startswith(b'\x7fELF') or path.suffix in ('.AppImage', '.flatpak'):
                # Scan paths in raw bytes and secrets in printable binary strings.
                if not reviewed_paths and privacy.private_paths(data):
                    failures.append(f'Private home path in binary: {prefix}/{relative}')
                data = subprocess.check_output(['strings', '-a', str(path)])
            record(f'{prefix}/{relative}', data, reviewed_paths=reviewed_paths)
    with tarfile.open(export / 'application-source.tar.gz', 'r:gz') as archive:
        for member in archive:
            if member.isdir():
                continue
            name = PurePosixPath(member.name).as_posix()
            if (not member.isfile() or PurePosixPath(name).is_absolute() or '..' in PurePosixPath(name).parts
                    or privacy.forbidden(name) or privacy.private_paths(os.fsencode(name))):
                failures.append(f'Unsafe source archive entry: {member.name}')
                continue
            record(f'source/{name}', archive.extractfile(member).read(), name)
    config = temporary / 'gitleaks.toml'
    config.write_text('[extend]\nuseDefault = true\n')
    (temporary / 'empty-ignore').touch()
    # Keep raw findings only inside the temporary directory; report hashes and filenames.
    executable = privacy.shutil.which('gitleaks')
    if not executable or subprocess.check_output([executable, 'version'], text=True).strip() != privacy.GITLEAKS_VERSION:
        raise ValueError('Pinned gitleaks is required')
    report = temporary / 'findings.json'
    result = subprocess.run([executable, 'dir', str(scan), '--config', str(config), '--no-banner',
                             '--gitleaks-ignore-path', str(temporary / 'empty-ignore'),
                             '--ignore-gitleaks-allow', '--report-format', 'json', '--report-path', str(report),
                             '--max-decode-depth', '5', '--max-archive-depth', '3'],
                            cwd=temporary, capture_output=True,
                            env={key: value for key, value in os.environ.items() if not key.startswith('GITLEAKS_')})
    if result.returncode not in (0, 1) or not report.exists():
        raise ValueError('Package secret scanner failed')
    reviewed = json.loads((ROOT / 'packaging/linux/reviewed-library-findings.json').read_text())
    allowed = {(row['path'], row['rule'], row['findingDigest']) for row in reviewed}
    findings = json.loads(report.read_text())
    for finding in findings:
        name = names[Path(finding['File']).name]
        digest = hashlib.sha256(finding['Secret'].encode()).hexdigest()
        if (name, finding['RuleID'], digest) not in allowed:
            failures.append(f"Unreviewed {finding['RuleID']}: {name}, finding SHA256 {digest}")
    if failures:
        raise ValueError('\n'.join(failures))
    print(f'Package privacy passed: {len(names)} files/string sets, {len(findings)} reviewed library findings')


def main(format, export):
    subprocess.run(['bash', str(ROOT / 'packaging/linux/validate-export.sh'), format, str(export)], check=True)
    with tempfile.TemporaryDirectory(prefix='tagrove-package-privacy-') as directory:
        temporary = Path(directory)
        if format == 'appimage':
            bundle, = export.glob('Tagrove-*.AppImage')
            subprocess.run([str(bundle), '--appimage-extract'], cwd=temporary, check=True, stdout=subprocess.DEVNULL)
            extracted = temporary / 'squashfs-root'
        else:
            bundle, = export.glob('Tagrove-*.flatpak')
            repo = temporary / 'ostree'
            subprocess.run(['ostree', 'init', f'--repo={repo}', '--mode=archive-z2'], check=True)
            subprocess.run(['flatpak', 'build-import-bundle', str(repo), str(bundle)], check=True)
            refs = subprocess.check_output(['ostree', f'--repo={repo}', 'refs'], text=True).splitlines()
            if len(refs) != 1:
                raise ValueError('Expected exactly one Flatpak ref')
            extracted = temporary / 'contents'
            subprocess.run(['ostree', f'--repo={repo}', 'checkout', '--user-mode', refs[0], str(extracted)], check=True)
        audit(extracted, export, temporary)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('format', choices=['appimage', 'flatpak'])
    parser.add_argument('export', type=Path)
    args = parser.parse_args()
    try:
        main(args.format, args.export.resolve())
    except (ValueError, OSError, subprocess.CalledProcessError, tarfile.TarError) as error:
        parser.exit(1, f'Package privacy failed: {error}\n')
