#!/usr/bin/env python3
"""Guard the lock file of the GPUI workspace (ADR 0036).

The backend core `media_tagger_core` compiles in two Cargo workspaces, each with its
own lock file. This check fails when they would build different cores or when the GPUI
lock gains a source nobody reviewed:

1. Core parity. Every package of the dependency closure of `media_tagger_core` that both
   locks contain in the same semver-compatible line has the same version, source and
   checksum in `src-tauri/Cargo.lock` and `src-gpui/Cargo.lock` (one bundled SQLite in
   both applications, ADR 0032). A package that only one closure contains comes from
   Cargo feature unification and is listed, not failed.
2. Sources. Every package of `src-gpui/Cargo.lock` comes from crates.io, from the Zed
   repository at the one `rev` pinned in `src-gpui/Cargo.toml`, or from a Git repository
   that this Zed commit pins itself (`PINNED_BY_ZED`). `gpui-pre` and `gpui-component`
   are refused under any source.
"""
import argparse
from pathlib import Path
import re
import sys
import tomllib

CORE = 'media_tagger_core'
CRATES_IO = 'registry+https://github.com/rust-lang/crates.io-index'
ZED = 'https://github.com/zed-industries/zed'
# Git dependencies of the pinned Zed commit (its workspace Cargo.toml and
# crates/gpui_linux/Cargo.toml). Review this list on every Zed rev bump.
PINNED_BY_ZED = {
    'https://github.com/gfx-rs/wgpu': 'cddb2f5d80dc0009b2ef6767635140cacf90a007',
    'https://github.com/zed-industries/font-kit': '94b0f28166665e8fd2f53ff6d268a14955c82269',
    'https://github.com/zed-industries/oo7': '731c1215edb6905876c6137ab300428c7b8d7c13',
    'https://github.com/zed-industries/scap': 'b5c86ec58dd4b6b7732bf6ac8be03d6b1f190a82',
    'https://github.com/zed-industries/xim-rs.git': '16f35a2c881b815a2b6cdfd6687988e84f8447d8',
}
REFUSED_PACKAGES = ('gpui-pre', 'gpui_pre', 'gpui-component', 'gpui_component')
FULL_REV = re.compile(r'[0-9a-f]{40}')


def load_packages(lock_path):
    return tomllib.loads(Path(lock_path).read_text())['package']


def compat_line(version):
    """The semver-compatible line of a version: 1.x.y -> '1', 0.4.x -> '0.4', 0.0.3 -> '0.0.3'."""
    major, minor, patch = re.split(r'[-+]', version, maxsplit=1)[0].split('.')[:3]
    if major != '0':
        return major
    return f'0.{minor}' if minor != '0' else f'0.0.{patch}'


def describe(package):
    return f"{package['version']} ({package.get('source', 'path')}, {package.get('checksum', 'no checksum')})"


def core_closure(packages, label):
    """Packages reachable from the core in one lock, keyed by (name, compatible line)."""
    by_name = {}
    for package in packages:
        by_name.setdefault(package['name'], []).append(package)

    def resolve(reference):
        # "name", "name version" or "name version (source)".
        name, _, rest = reference.partition(' ')
        version, _, source = rest.partition(' ')
        matches = [candidate for candidate in by_name.get(name, [])
                   if (not version or candidate['version'] == version)
                   and (not source or candidate.get('source') == source.strip('()'))]
        if len(matches) != 1:
            raise ValueError(f'{label}: cannot resolve dependency "{reference}"')
        return matches[0]

    cores = by_name.get(CORE, [])
    if len(cores) != 1 or 'source' in cores[0]:
        raise ValueError(f'{label}: expected {CORE} once, as a path package')
    closure = {}
    pending = [resolve(reference) for reference in cores[0].get('dependencies', [])]
    if not pending:
        raise ValueError(f'{label}: {CORE} has no dependencies')
    while pending:
        package = pending.pop()
        key = (package['name'], compat_line(package['version']))
        known = closure.get(key)
        if known is package:
            continue
        if known is not None:
            raise ValueError(f"{label}: two {package['name']} packages in one compatible line")
        closure[key] = package
        pending.extend(resolve(reference) for reference in package.get('dependencies', []))
    return closure


def parity(tauri_packages, gpui_packages):
    """(problems, notes) of the core closures of both locks."""
    tauri = core_closure(tauri_packages, 'src-tauri/Cargo.lock')
    gpui = core_closure(gpui_packages, 'src-gpui/Cargo.lock')
    problems = []
    for key in sorted(tauri.keys() & gpui.keys()):
        left, right = tauri[key], gpui[key]
        if any(left.get(field) != right.get(field) for field in ('version', 'source', 'checksum')):
            problems.append(f'{key[0]}: src-tauri/Cargo.lock has {describe(left)}, '
                            f'src-gpui/Cargo.lock has {describe(right)}')
    notes = []
    for label, closure, other in (('src-gpui/Cargo.lock', gpui, tauri), ('src-tauri/Cargo.lock', tauri, gpui)):
        only = sorted(f"{key[0]} {closure[key]['version']}" for key in closure.keys() - other.keys())
        if only:
            notes.append(f'only in the core closure of {label} (feature unification or dev-dependencies): '
                         + ', '.join(only))
    return problems, notes, len(tauri.keys() & gpui.keys())


def pinned_zed_rev(manifest_path):
    """The one full rev every Zed dependency of the GPUI workspace manifest names."""
    manifest = tomllib.loads(Path(manifest_path).read_text())
    dependencies = manifest.get('workspace', {}).get('dependencies', {})
    revs = set()
    for name, value in dependencies.items():
        if not isinstance(value, dict) or 'git' not in value:
            continue
        if value['git'] != ZED:
            raise ValueError(f"{manifest_path}: {name} comes from {value['git']}; only {ZED} is allowed")
        if set(value) & {'branch', 'tag'} or not FULL_REV.fullmatch(value.get('rev', '')):
            raise ValueError(f'{manifest_path}: {name} must pin a full 40-character rev')
        revs.add(value['rev'])
    if len(revs) != 1:
        raise ValueError(f'{manifest_path}: expected one pinned Zed rev, found {sorted(revs) or "none"}')
    return revs.pop()


def source_problems(gpui_packages, zed_rev):
    allowed = {ZED: zed_rev, **PINNED_BY_ZED}
    problems = []
    for package in gpui_packages:
        name, source = package['name'], package.get('source')
        if name in REFUSED_PACKAGES:
            problems.append(f'{name}: third-party GPUI snapshots and component libraries are refused')
        if source is None or source == CRATES_IO:
            continue
        match = re.fullmatch(r'git\+([^?#]+)\?rev=([0-9a-f]+)#([0-9a-f]{40})', source)
        if not match:
            problems.append(f'{name}: unsupported source {source}')
            continue
        url, rev, commit = match.groups()
        if url not in allowed:
            problems.append(f'{name}: Git source {url} is neither Zed nor pinned by Zed')
        elif commit != allowed[url] or rev != commit:
            problems.append(f'{name}: {url} is locked at {commit} (rev {rev}), expected {allowed[url]}')
    return problems


def check(root):
    tauri_packages = load_packages(root / 'src-tauri/Cargo.lock')
    gpui_packages = load_packages(root / 'src-gpui/Cargo.lock')
    problems, notes, compared = parity(tauri_packages, gpui_packages)
    zed_rev = pinned_zed_rev(root / 'src-gpui/Cargo.toml')
    problems += source_problems(gpui_packages, zed_rev)
    return problems, notes, compared, zed_rev


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        found, remarks, total, rev = check(args.repo)
    except (ValueError, OSError, KeyError, tomllib.TOMLDecodeError) as error:
        sys.exit(f'GPUI lock check failed: {error}')
    for remark in remarks:
        print(f'note: {remark}')
    if found:
        sys.exit('GPUI lock check failed:\n' + '\n'.join(f'  {problem}' for problem in found)
                 + '\nAlign src-gpui/Cargo.lock with: cargo update --manifest-path src-gpui/Cargo.toml'
                   ' -p <name> --precise <version of src-tauri/Cargo.lock>')
    print(f'GPUI lock check passed: {total} core packages identical in both locks; Zed pinned at {rev}')
