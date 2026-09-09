#!/usr/bin/env python3
"""Fail closed on private filenames, home paths and secrets in publishable Git data."""
import argparse
import fnmatch
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

GITLEAKS_VERSION = '8.30.1'
FORBIDDEN = ('.env', '.env.*', '.npmrc', '.pypirc', '*.key', '*.pem', '*.p12', '*.pfx',
             'id_rsa*', 'id_ed25519*', 'id_ecdsa*', '*.db', '*.db-*', '*.sqlite*',
             '*.zip', '*.7z', '*.tar', '*.tar.gz', '*.tgz', '*.tsbuildinfo', '*.log')
LOCAL_DIRECTORIES = {'node_modules', 'artifacts', 'output', 'thumbs', 'restore-staging'}
HOME_PATH = re.compile(rb"""(?<![A-Za-z0-9_-])(?:/(?:home|Users)/[^/\s<>"'\x00]+|[A-Za-z]:[\\/]Users[\\/][^/\\\s<>"'\x00]+)(?:[/\\]|(?=$|[\s\x00"'<>]))""")
# Exact illustrative paths in retained documentation/tests, not machine identities.
EXAMPLES = {
    '.agents/skills/recall/SKILL.md': {b'/Users/' + b'you/'},
    'src-tauri/src/services/backup_service.rs': {b'C:/Users/' + b'Example/', b'C:\\Users\\' + b'Example\\'},
}


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args])


def forbidden(name):
    parts = Path(name).parts
    return (any(fnmatch.fnmatchcase(parts[-1].lower(), pattern) for pattern in FORBIDDEN)
            or bool(LOCAL_DIRECTORIES.intersection(parts))
            or parts[-1] == 'instance.lock'
            or name.startswith(('src-tauri/target/', 'src-tauri/target-')))


def private_paths(data, names=()):
    return [match.group() for match in HOME_PATH.finditer(data)
            if not names or not all(match.group().rstrip(b'/\\') in {item.rstrip(b'/\\') for item in EXAMPLES.get(name, set())} for name in names)]


def tree(root, revision):
    entries = []
    for row in git(root, 'ls-tree', '-rz', revision).split(b'\0'):
        if row:
            header, name = row.split(b'\t', 1)
            mode, kind, oid = header.decode().split()
            entries.append((mode, kind, oid, os.fsdecode(name)))
    return entries


def scan_secrets(directory, config):
    executable = shutil.which('gitleaks')
    if not executable or subprocess.check_output([executable, 'version'], text=True).strip() != GITLEAKS_VERSION:
        raise ValueError(f'gitleaks {GITLEAKS_VERSION} is required on PATH')
    # Explicit configuration and empty ignore file prevent local bypasses.
    subprocess.run([executable, 'dir', str(directory), '--config', str(config),
                    '--gitleaks-ignore-path', str(config.parent / 'empty-ignore'),
                    '--ignore-gitleaks-allow', '--redact', '--no-banner',
                    '--max-decode-depth', '5', '--max-archive-depth', '3'],
                   check=True, cwd=config.parent,
                   env={key: value for key, value in os.environ.items() if not key.startswith('GITLEAKS_')})


def check(root):
    if git(root, 'rev-parse', '--is-shallow-repository').strip() != b'false':
        raise ValueError('Full history required; fetch with depth 0')
    if git(root, 'for-each-ref', '--format=%(refname)', 'refs/replace').strip():
        raise ValueError('Replace refs can conceal history')
    if (root / os.fsdecode(git(root, 'rev-parse', '--git-path', 'info/grafts').strip())).exists():
        raise ValueError('Grafts can conceal history')
    failures = set()
    names_by_oid = {}
    commits = git(root, 'rev-list', '--all').decode().splitlines()
    for commit in commits:
        for mode, kind, oid, name in tree(root, commit):
            if kind != 'blob':
                failures.add(f'Unsupported Git entry: {commit}:{name}')
            if forbidden(name) or private_paths(os.fsencode(name)):
                failures.add(f'Forbidden historical filename: {commit}:{name}')
            names_by_oid.setdefault(oid, set()).add(name)
    with tempfile.TemporaryDirectory(prefix='tagrove-privacy-') as temporary:
        temp = Path(temporary)
        objects = temp / 'objects'
        objects.mkdir()
        config = temp / 'gitleaks.toml'
        config.write_text('[extend]\nuseDefault = true\n')
        (temp / 'empty-ignore').touch()
        oids = git(root, 'rev-list', '--objects', '--all', '--no-object-names').decode().splitlines()
        for row in git(root, 'ls-files', '--stage', '-z').split(b'\0'):
            if not row:
                continue
            header, name = row.split(b'\t', 1)
            mode, oid, stage = header.decode().split()
            if stage != '0' or mode not in ('100644', '100755', '120000'):
                raise ValueError('Unmerged or unsupported index entry')
            name = os.fsdecode(name)
            names_by_oid.setdefault(oid, set()).add(name)
            if oid not in oids:
                oids.append(oid)
        # Full versions, not only added diff lines. Include commit/tag metadata and tree names.
        batch = subprocess.Popen(['git', '-C', str(root), 'cat-file', '--batch'],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        try:
            for oid in oids:
                batch.stdin.write((oid + '\n').encode())
                batch.stdin.flush()
                header = batch.stdout.readline().split()
                if len(header) != 3:
                    raise ValueError('Missing Git object')
                data = batch.stdout.read(int(header[2]))
                if batch.stdout.read(1) != b'\n':
                    raise ValueError('Invalid Git object framing')
                if private_paths(data, names_by_oid.get(oid, ())):
                    failures.add(f'Private home path in object {oid}')
                (objects / oid).write_bytes(data)
        finally:
            batch.stdin.close()
            batch.stdout.close()
            if batch.wait() != 0:
                raise ValueError('Cannot read complete history')
        for name in os.fsdecode(git(root, 'ls-files', '-z')).split('\0'):
            if not name:
                continue
            if forbidden(name) or private_paths(os.fsencode(name)):
                failures.add(f'Forbidden tracked filename: {name}')
            path = root / name
            if not path.exists() and not path.is_symlink():
                continue  # Deleted locally; the historical object was scanned above.
            data = os.fsencode(os.readlink(path)) if path.is_symlink() else path.read_bytes()
            if private_paths(data, [name]):
                failures.add(f'Private home path in tracked file: {name}')
            (objects / ('working-' + hashlib.sha256(os.fsencode(name)).hexdigest())).write_bytes(data)
        refs = git(root, 'for-each-ref', '--format=%(refname) %(contents)')
        if private_paths(refs):
            failures.add('Private home path in reference metadata')
        (objects / 'references').write_bytes(refs)
        scan_secrets(objects, config)
    if failures:
        raise ValueError('\n'.join(sorted(failures)))
    print(f'Privacy check passed: {len(commits)} commits, {len(oids)} complete objects and tracked working files')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path.cwd())
    args = parser.parse_args()
    try:
        check(args.repo.resolve())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'Privacy check failed: {error}\n')
