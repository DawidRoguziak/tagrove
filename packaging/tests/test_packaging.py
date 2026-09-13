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
            for fault in [None, 'changed', 'symlink', 'duplicate', 'extra-root', 'publisher', 'release-fields']:
                with self.subTest(fault=fault):
                    with tarfile.open(archive_path, 'w:gz') as archive:
                        for name in [*verify_release.REQUIRED_INPUTS, verify_release.PUBLISHER_INPUT]:
                            data = (ROOT / name).read_bytes()
                            if name == verify_release.PUBLISHER_INPUT:
                                publisher = json.loads(data)
                                if fault == 'publisher':
                                    publisher['developerName'] = 'Different publisher'
                                if fault == 'release-fields':
                                    publisher['releaseSha256'] = 'a' * 64
                                data = json.dumps(publisher).encode()
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
                    if fault and fault != 'release-fields':
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
            generate.generate(ROOT, dict(self.publisher, appId='com.example.mediatagger'), True)
        for app_id in ['com.example.mediatagger.e2e', 'com.example.mediatagger.dev', 'bad/id',
                       'Org.tagrove.Tagrove', 'org.tagróve.Tagrove', 'org.a.b.c.d.Tagrove',
                       'org.tagrove.1App', 'org.' + 'a' * 250 + '.Tagrove']:
            with self.subTest(app_id=app_id), self.assertRaises(ValueError):
                generate.validate_publisher(dict(self.publisher, appId=app_id))
        generate.validate_publisher(dict(self.publisher, appId='org.tagrove.Tagrove-player'))

    def test_publisher_identity_local_profile_and_missing_release(self):
        publisher = self.publisher
        repository = generate.urlparse(publisher['repository'])
        owner, product = repository.path.strip('/').split('/')
        self.assertEqual(publisher['appId'], f'io.github.{owner.lower()}.{product.lower()}')
        self.assertEqual(publisher['developerId'], publisher['appId'])
        self.assertEqual(publisher['developerName'], 'Tagrove')
        with self.assertRaises(ValueError) as error:
            generate.generate(ROOT, publisher, True)
        for missing in ('releaseUrl', 'releaseRef', 'releaseDate', 'releaseSha256', 'screenshots'):
            self.assertIn(missing, str(error.exception))
        complete = dict(publisher, releaseUrl=publisher['repository'] + '/archive/refs/tags/v0.1.1.tar.gz',
                        releaseRef='v0.1.1', releaseDate='2026-09-13', releaseSha256='a'*64,
                        screenshots=[publisher['repository'] + '/raw/v0.1.1/gallery.png'])
        local_id = json.loads((ROOT / 'src-tauri/tauri.conf.json').read_text())['identifier']
        for publication, expected_id in ((False, local_id), (True, publisher['appId'])):
            manifest = generate.generate(ROOT, complete, publication)
            self.assertEqual(manifest['app-id'], expected_id)
            sources = {s['dest-filename']: s['contents'] for s in manifest['modules'][-1]['sources']
                       if s['type'] == 'inline'}
            self.assertEqual(json.loads(sources['tauri.flatpak.generated.json'])['identifier'], expected_id)
            self.assertEqual(json.loads(sources['publisher.json'])['appId'], expected_id)
            self.assertIn('Icon=' + expected_id, sources['tagrove.desktop'])
            metadata = generate.ET.fromstring(sources['tagrove.metainfo.xml'])
            self.assertEqual(metadata.findtext('id'), expected_id)
            self.assertEqual(metadata.findtext('launchable'), expected_id + '.desktop')
            self.assertEqual(metadata.findtext('developer/name'), publisher['developerName'])
            self.assertEqual(metadata.find('developer').attrib['id'], publisher['developerId'])
            self.assertEqual(metadata.findtext('url'), publisher['repository'])
            self.assertEqual(metadata.find('releases/release').attrib['version'],
                             json.loads((ROOT / 'package.json').read_text())['version'])
            self.assertNotIn('--share=network', manifest['finish-args'])
        generate.validate_publisher(dict(complete, repository=publisher['repository'].upper().replace('HTTPS', 'https')), True)
        with self.assertRaisesRegex(ValueError, 'requires repository'):
            generate.validate_publisher(dict(complete, repository='https://github.com/another/tagrove'), True)
        self.assertEqual(self.publisher['appId'], publisher['appId'])

    def test_opener_is_scoped_to_the_shared_repository(self):
        capability = json.loads((ROOT / 'src-tauri/capabilities/default.json').read_text())
        permissions = [p for p in capability['permissions']
                       if (p if isinstance(p, str) else p['identifier']).startswith('opener:')]
        self.assertEqual(permissions, [{'identifier': 'opener:allow-open-url', 'allow': [
            {'url': self.publisher['repository']}, {'url': self.publisher['repository'] + '/*'}]}])

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
    def prepare_build_repo(self, root):
        for name in ['.gitignore', 'scripts/privacy-check.py', 'packaging/linux/prepare-source.py']:
            target = root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / name, target)
        for args in [('init', '-q'), ('config', 'user.name', 'Test'),
                     ('config', 'user.email', 'test@example.invalid'), ('add', '.'),
                     ('commit', '-qm', 'fixture')]:
            subprocess.run(['git', '-C', str(root), *args], check=True, capture_output=True)

    def fixture(self, root):
        for name, contents in {'build-manifest.txt': 'format=flatpak\narchitecture=x86_64\ncompile-network=none\nsource-commit=' + 'a'*40 + '\n',
                               'source-commit.txt': 'a'*40 + '\n',
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
        for name in ['toolchains.json', 'appimage-tools.json']:
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
            docker.write_text('#!/bin/sh\necho docker-build-attempted >&2\nexit 42\n')
            docker.chmod(0o755)
            self.prepare_build_repo(root)
            result = subprocess.run(['bash', str(root / 'scripts/build-linux-docker.sh'), '--format', 'flatpak'],
                                    env=dict(os.environ, PATH=str(fake_bin) + os.pathsep + os.environ['PATH']), capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b'docker-build-attempted', result.stderr)
            self.assertEqual(previous.read_bytes(), b'previous successful package')
            self.assertEqual(list((root / 'artifacts').glob('.linux.*')), [])

    def test_native_export_checks_archive_contents_and_architecture(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            (root / 'Tagrove-1.0-x86_64.flatpak').unlink()
            (root / 'flatpak-manifest.json').unlink()
            (root / 'build-manifest.txt').write_text('format=native\narchitecture=x86_64\nsource-commit=' + 'a'*40 + '\n')
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
                    (fixture / 'flatpak-manifest.json').unlink()
                    (fixture / 'debian-sources.tsv').write_text('fixture')
                    shutil.copy('/usr/bin/true', fixture / 'Tagrove-1.0-x86_64.AppImage')
                    (fixture / 'build-manifest.txt').write_text('format=appimage\narchitecture=x86_64\nsource-commit=' + 'a'*40 + '\n')
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
  cp)
    cp -a "$TEST_FIXTURES/${2%%:*}/." "$3"
    cp "$TEST_PROJECT"/artifacts/.linux.*/context/.release-source/{application-source.tar.gz,source-commit.txt} "$3/"
    sed -i '/^source-commit=/d' "$3/build-manifest.txt"
    echo "source-commit=$(cat "$3/source-commit.txt")" >> "$3/build-manifest.txt"
    (cd "$3" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS) ;;
esac
exit 0
''')
            docker.chmod(0o755)
            move = fake_bin / 'mv'
            move.write_text('''#!/bin/bash
if [[ "$2" == */artifacts/.linux.*/appimage && "$3" == */artifacts/linux/appimage ]]; then echo rollback-triggered >&2; exit 42; fi
exec /usr/bin/mv "$@"
''')
            move.chmod(0o755)
            self.prepare_build_repo(root)
            result = subprocess.run(['bash', str(root / 'scripts/build-linux-docker.sh')], capture_output=True,
                                    env=dict(os.environ, PATH=str(fake_bin) + os.pathsep + os.environ['PATH'], TEST_FIXTURES=str(fixtures), TEST_PROJECT=str(root)))
            self.assertIn(b'rollback-triggered', result.stderr)
            self.assertNotEqual(result.returncode, 0)
            for format in ['flatpak', 'appimage']:
                output = root / 'artifacts/linux' / format
                self.assertEqual((output / 'previous').read_text(), 'original ' + format)
                self.assertEqual([path.name for path in output.iterdir()], ['previous'])


if __name__ == '__main__':
    unittest.main()
