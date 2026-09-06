import copy
import hashlib
import importlib.util
import json
import os
import io
from pathlib import Path
import shutil
import subprocess
import tempfile
import tarfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('generate', ROOT / 'packaging/flatpak/generate.py')
generate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generate)
spec = importlib.util.spec_from_file_location('verify_release', ROOT / 'packaging/flatpak/verify-release.py')
verify_release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify_release)


class ReleaseArchiveTests(unittest.TestCase):
    def test_public_archive_matches_checkout_and_rejects_ambiguous_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            archive_path = Path(temporary) / 'release.tar.gz'
            for fault in [None, 'changed', 'symlink', 'duplicate', 'extra-root']:
                with self.subTest(fault=fault):
                    with tarfile.open(archive_path, 'w:gz') as archive:
                        for name in verify_release.REQUIRED_INPUTS:
                            data = (ROOT / name).read_bytes()
                            member = tarfile.TarInfo('release/' + name)
                            if name == 'package.json' and fault == 'changed':
                                data += b'changed'
                            if name == 'package.json' and fault == 'symlink':
                                member.type = tarfile.SYMTYPE
                                member.linkname = '../package.json'
                                archive.addfile(member)
                                continue
                            member.size = len(data)
                            archive.addfile(member, io.BytesIO(data))
                            if name == 'package.json' and fault == 'duplicate':
                                archive.addfile(member, io.BytesIO(data))
                        if fault == 'extra-root':
                            archive.addfile(tarfile.TarInfo('elsewhere/input'))
                    if fault:
                        with self.assertRaises(ValueError):
                            verify_release.verify_archive(ROOT, archive_path)
                    else:
                        verify_release.verify_archive(ROOT, archive_path)


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.publisher = json.loads((ROOT / 'packaging/flatpak/publisher.json').read_text())

    def test_sources_match_lockfile_integrity_and_offline_build(self):
        manifest = generate.generate(ROOT, self.publisher)
        sources = manifest['modules'][-1]['sources']
        archives = {source['url']: source for source in sources if source['type'] == 'archive'}
        self.assertGreater(len(archives), 500)
        cargo = generate.tomllib.loads((ROOT / 'src-tauri/Cargo.lock').read_text())
        for package in cargo['package']:
            if 'source' in package:
                url = f"https://static.crates.io/crates/{package['name']}/{package['name']}-{package['version']}.crate"
                self.assertEqual(archives[url]['sha256'], package['checksum'])
        self.assertNotIn('--share=network', manifest['finish-args'])
        self.assertEqual(manifest['runtime-version'], '50')
        self.assertIn('--unshare=network', manifest['build-options']['build-args'])
        self.assertEqual(manifest['build-options']['env']['CARGO_NET_OFFLINE'], 'true')

    def test_publication_rejects_local_and_profile_identities(self):
        with self.assertRaisesRegex(ValueError, 'placeholder'):
            generate.generate(ROOT, self.publisher, True)
        for app_id in ['com.example.mediatagger.e2e', 'com.example.mediatagger.dev', 'bad/id',
                       'Org.tagrove.Tagrove', 'org.tagróve.Tagrove', 'org.a.b.c.d.Tagrove',
                       'org.tagrove.1App', 'org.' + 'a' * 250 + '.Tagrove']:
            with self.subTest(app_id=app_id), self.assertRaises(ValueError):
                generate.validate_publisher(dict(self.publisher, appId=app_id))
        generate.validate_publisher(dict(self.publisher, appId='org.tagrove.Tagrove-player'))

    def test_publication_shares_modules_and_uses_checksummed_release(self):
        publisher = dict(self.publisher, appId='org.tagrove.Tagrove', repository='https://github.com/tagrove/tagrove',
                         releaseUrl='https://github.com/tagrove/tagrove/archive/v1.0.tar.gz', releaseSha256='a'*64,
                         releaseRef='v1.0', releaseDate='2026-09-06', developerId='tagrove', developerName='Tagrove contributors',
                         screenshots=['https://tagrove.org/gallery.png'])
        local = generate.generate(ROOT, publisher)
        publication = generate.generate(ROOT, publisher, True)
        self.assertEqual(local['modules'][:-1], publication['modules'][:-1])
        self.assertEqual(publication['modules'][-1]['sources'][0]['sha256'], 'a'*64)
        self.assertFalse(any(source['type'] == 'dir' for source in publication['modules'][-1]['sources']))
        generate.validate_publisher(dict(publisher, appId='io.github.tagrove.tagrove'), True)
        with self.assertRaisesRegex(ValueError, 'requires repository'):
            generate.validate_publisher(dict(publisher, appId='io.github.someone.Tagrove'), True)
        for url in ['file:///tmp/source.tar.gz', 'https://127.0.0.1/source.tar.gz', 'https://example.com/a', 'https://host.local/a']:
            with self.subTest(url=url), self.assertRaises(ValueError):
                generate.generate(ROOT, dict(publisher, releaseUrl=url), True)

    def test_jsonc_parser_preserves_strings(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'bun.lock'
            path.write_text('{"string": ", }", "array": [1,],}')
            self.assertEqual(generate.read_bun_lock(path), {'string': ', }', 'array': [1]})


class ExportTests(unittest.TestCase):
    def fixture(self, root):
        for name, contents in {'build-manifest.txt': 'format=flatpak\narchitecture=x86_64\ncompile-network=none\n',
                               'LICENSE': 'license', 'application-source.tar.gz': 'source', 'SOURCE-NOTICE.txt': 'notice',
                               'flatpak-manifest.json': '{}', 'Tagrove-1.0-x86_64.flatpak': 'fixture'}.items():
            (root / name).write_text(contents)
        (root / 'licenses/dependencies').mkdir(parents=True)
        (root / 'licenses/dependencies/index.json').write_text('[{\"fixture\": true}]')
        for name in ['libass/LICENSE', 'ffmpeg/LICENSE', 'mpv/LICENSE', 'libplacebo/LICENSE',
                     'libplacebo/3rdparty/fast_float/LICENSE-MIT', 'libplacebo/3rdparty/glad/LICENSE',
                     'libplacebo/3rdparty/jinja/LICENSE.txt', 'libplacebo/3rdparty/markupsafe/LICENSE.txt',
                     'appimage/AppImage-runtime.txt', 'appimage/AppImageKit.txt', 'appimage/SOURCES.json']:
            path = root / 'licenses' / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('fixture notice')
        for name in ['debian-sources.tsv', 'toolchains.json', 'appimage-tools.json']:
            (root / name).write_text('fixture source inventory')
        self.checksums(root)

    def checksums(self, root):
        (root / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  ./{path.relative_to(root)}\n'
                                               for path in sorted(root.rglob('*')) if path.is_file() and path.name != 'SHA256SUMS'))

    def validate(self, root):
        return subprocess.run(['bash', str(ROOT / 'packaging/linux/validate-export.sh'), 'flatpak', str(root)], capture_output=True)

    def test_export_rejects_corruption_unlisted_files_and_duplicate_bundles(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            self.assertEqual(self.validate(root).returncode, 0)
            bundle = root / 'Tagrove-1.0-x86_64.flatpak'
            bundle.write_text('corrupted')
            self.assertNotEqual(self.validate(root).returncode, 0)
            self.checksums(root)
            extra = root / 'Tagrove-2.0-x86_64.flatpak'
            extra.write_text('second')
            self.assertNotEqual(self.validate(root).returncode, 0)
            self.checksums(root)
            self.assertNotEqual(self.validate(root).returncode, 0)

    def test_failed_build_preserves_previous_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'scripts').mkdir()
            shutil.copy(ROOT / 'scripts/build-linux-docker.sh', root / 'scripts')
            previous = root / 'artifacts/linux/flatpak/Tagrove-previous.flatpak'
            previous.parent.mkdir(parents=True)
            previous.write_bytes(b'previous successful package')
            fake_bin = root / 'bin'
            fake_bin.mkdir()
            docker = fake_bin / 'docker'
            docker.write_text('#!/bin/sh\nexit 42\n')
            docker.chmod(0o755)
            result = subprocess.run(['bash', str(root / 'scripts/build-linux-docker.sh'), '--format', 'flatpak'],
                                    env=dict(os.environ, PATH=str(fake_bin) + os.pathsep + os.environ['PATH']), capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(previous.read_bytes(), b'previous successful package')
            self.assertEqual(list((root / 'artifacts').glob('.linux.*')), [])

    def test_native_export_checks_archive_contents_and_architecture(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            (root / 'Tagrove-1.0-x86_64.flatpak').unlink()
            (root / 'build-manifest.txt').write_text('format=native\narchitecture=x86_64\n')
            for fault in [None, 'not-elf', 'symlink', 'extra-file']:
                with self.subTest(fault=fault):
                    with tarfile.open(root / 'Tagrove-1.0-x86_64-native.tar.gz', 'w:gz') as archive:
                        binary = Path('/usr/bin/true').read_bytes() if fault != 'not-elf' else b'not an executable'
                        member = tarfile.TarInfo('./media_tagger')
                        member.mode = 0o755
                        member.size = len(binary)
                        if fault == 'symlink':
                            member.type = tarfile.SYMTYPE
                            member.linkname = '/usr/bin/true'
                        archive.addfile(member, io.BytesIO(binary))
                        archive.add(ROOT / 'src-tauri/icons/512x512.png', './tagrove.png')
                        archive.add(ROOT / 'src-tauri/linux/com.example.mediatagger.desktop', './com.example.mediatagger.desktop')
                        if fault == 'extra-file':
                            archive.addfile(tarfile.TarInfo('../unexpected'))
                    self.checksums(root)
                    result = subprocess.run(['bash', str(ROOT / 'packaging/linux/validate-export.sh'),
                                             'native', str(root)], capture_output=True)
                    self.assertEqual(result.returncode == 0, fault is None, result.stderr.decode())

    def test_late_export_failure_restores_all_formats(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'scripts').mkdir()
            shutil.copy(ROOT / 'scripts/build-linux-docker.sh', root / 'scripts')
            (root / 'packaging/linux').mkdir(parents=True)
            shutil.copy(ROOT / 'packaging/linux/validate-export.sh', root / 'packaging/linux')
            fixtures = root / 'fixtures'
            for format in ['flatpak', 'appimage']:
                fixture = fixtures / format
                fixture.mkdir(parents=True)
                self.fixture(fixture)
                if format == 'appimage':
                    (fixture / 'Tagrove-1.0-x86_64.flatpak').unlink()
                    shutil.copy('/usr/bin/true', fixture / 'Tagrove-1.0-x86_64.AppImage')
                    (fixture / 'build-manifest.txt').write_text('format=appimage\narchitecture=x86_64\n')
                    (fixture / 'runtime-check.txt').write_text('fixture runtime check')
                    self.checksums(fixture)
                previous = root / 'artifacts/linux' / format / 'previous'
                previous.parent.mkdir(parents=True)
                previous.write_text('original ' + format)
            fake_bin = root / 'bin'
            fake_bin.mkdir()
            docker = fake_bin / 'docker'
            docker.write_text('''#!/bin/bash
case "$1" in
  build)
    if [[ "$*" == *appimage* ]]; then image_id=appimage; else image_id=flatpak; fi
    while [[ $# -gt 0 ]]; do
      if [[ "$1" == --iidfile ]]; then echo "$image_id" > "$2"; break; fi
      shift
    done ;;
  volume) [[ "$2" != create ]] || echo test-volume ;;
  create) if [[ "$*" == *appimage* ]]; then echo appimage; else echo flatpak; fi ;;
  inspect) echo 0 ;;
  cp) cp -a "$TEST_FIXTURES/${2%%:*}/." "$3" ;;
esac
exit 0
''')
            docker.chmod(0o755)
            move = fake_bin / 'mv'
            move.write_text('''#!/bin/bash
if [[ "$2" == */artifacts/.linux.*/appimage && "$3" == */artifacts/linux/appimage ]]; then exit 42; fi
exec /usr/bin/mv "$@"
''')
            move.chmod(0o755)
            result = subprocess.run(['bash', str(root / 'scripts/build-linux-docker.sh')], capture_output=True,
                                    env=dict(os.environ, PATH=str(fake_bin) + os.pathsep + os.environ['PATH'], TEST_FIXTURES=str(fixtures)))
            self.assertNotEqual(result.returncode, 0)
            for format in ['flatpak', 'appimage']:
                output = root / 'artifacts/linux' / format
                self.assertEqual((output / 'previous').read_text(), 'original ' + format)
                self.assertEqual([path.name for path in output.iterdir()], ['previous'])


if __name__ == '__main__':
    unittest.main()
