import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('audit', ROOT / 'packaging/linux/audit-package.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class PackagePrivacyTests(unittest.TestCase):
    def run_audit(self, contents, name='note.txt', source_name='README.md', exported=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            extracted = root / 'extracted'
            export = root / 'export'
            temporary = root / 'temporary'
            for path in [extracted, export, temporary]:
                path.mkdir()
            path = (export if exported else extracted) / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(contents)
            with tarfile.open(export / 'application-source.tar.gz', 'w:gz') as archive:
                entry = tarfile.TarInfo(source_name)
                entry.size = 4
                archive.addfile(entry, io.BytesIO(b'safe'))
            audit.audit(extracted, export, temporary)

    def test_clean_package(self):
        self.run_audit(b'safe')

    def test_private_path_and_local_database(self):
        with self.assertRaisesRegex(ValueError, 'Private home path'):
            self.run_audit(b'/home/' + b'synthetic-person/project')
        for name in ['nested/media.db', 'private.key', 'nested/id_ed25519',
                     'nested/media.sqlite-wal', 'debug.log', 'archive.zip']:
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'Local application'):
                self.run_audit(b'synthetic local file', name)

    def test_bundle_wrapper_bytes(self):
        for extension in ['AppImage', 'flatpak']:
            with self.subTest(extension=extension), self.assertRaisesRegex(ValueError, 'Private home path'):
                self.run_audit(b'/home/' + b'synthetic-person/project',
                               'Tagrove-1.0-x86_64.' + extension, exported=True)

    def test_mime_definition_is_not_a_database(self):
        self.run_audit(b'<mime-type/>', 'usr/share/mime/application/vnd.sqlite3.xml')

    def test_secret_and_no_library_path_wildcard(self):
        token = b'token="' + b'ghp_' + b'A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8' + b'"'
        with self.assertRaisesRegex(ValueError, 'Unreviewed'):
            self.run_audit(token, 'usr/lib/libgnutls.so.30')
        with self.assertRaisesRegex(ValueError, 'Private home path'):
            self.run_audit(b'/home/' + b'synthetic-person/project', 'usr/lib/libgtk-3.so.0')

    def test_source_archive_rejects_local_files_and_escape(self):
        for name in ['nested/.env', '../escape', '/absolute', 'media.db']:
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'Unsafe source archive'):
                self.run_audit(b'safe', source_name=name)
