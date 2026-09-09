#!/usr/bin/env python3
"""Create an immutable Docker context and corresponding sources from a clean commit."""
import argparse
import gzip
import importlib.util
from pathlib import Path
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('privacy', ROOT / 'scripts/privacy-check.py')
privacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(privacy)


def prepare(root, destination, revision):
    commit = privacy.git(root, 'rev-parse', '--verify', revision + '^{commit}').decode().strip()
    if privacy.git(root, 'rev-parse', 'HEAD').decode().strip() != commit:
        raise ValueError('Checkout must be at the requested commit')
    if privacy.git(root, 'status', '--porcelain', '--untracked-files=all').strip():
        raise ValueError('Release requires a clean checkout; commit or remove local changes')
    if destination.exists():
        raise ValueError('Destination must not exist')
    privacy.check(root)
    entries = privacy.tree(root, commit)
    # These outputs have no place in a release tree, even when force-added.
    for mode, kind, _, name in entries:
        if (kind != 'blob' or mode not in ('100644', '100755') or name.startswith(
                ('.release-source/', 'dist/', 'src-tauri/gen/schemas/'))):
            raise ValueError(f'Unsupported release input: {name}')
    destination.mkdir(parents=True)
    metadata = destination / '.release-source'
    metadata.mkdir()
    archive = metadata / 'application-source.tar.gz'
    # git archive owns names, modes, timestamps and neutral owner metadata.
    raw = privacy.git(root, '-c', 'tar.umask=0022', 'archive', '--format=tar', commit)
    archive.write_bytes(gzip.compress(raw, mtime=0))
    with tarfile.open(archive, 'r:gz') as source:
        files = {item.name: item for item in source.getmembers() if not item.isdir()}
        if len(source.getmembers()) != len({item.name for item in source.getmembers()}):
            raise ValueError('Duplicate archive entries')
        if set(files) != {entry[3] for entry in entries}:
            raise ValueError('Archive attributes changed the committed file set')
        for mode, _, oid, name in entries:
            item = files[name]
            if (not item.isfile() or item.mode != int(mode, 8) & 0o777
                    or source.extractfile(item).read() != privacy.git(root, 'cat-file', 'blob', oid)):
                raise ValueError(f'Archive differs from Git: {name}')
        source.extractall(destination, filter='data')
    (metadata / 'source-commit.txt').write_text(commit + '\n')
    with (metadata / 'SHA256SUMS').open('w') as checksums:
        subprocess.run(['sha256sum', 'application-source.tar.gz'], cwd=metadata, check=True, stdout=checksums)
    print(f'Prepared release source {commit}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--commit', required=True)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    try:
        prepare(ROOT, args.destination.resolve(), args.commit)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'Source preparation failed: {error}\n')
