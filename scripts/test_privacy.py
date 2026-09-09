import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import unittest.mock

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('source', ROOT / 'packaging/linux/prepare-source.py')
source = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source)


class PrivacyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='tagrove-privacy-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'repository'
        self.root.mkdir()
        self.git('init', '-q')
        self.git('config', 'user.name', 'Test')
        self.git('config', 'user.email', 'test@example.invalid')
        (self.root / '.gitignore').write_bytes((ROOT / '.gitignore').read_bytes())
        self.write('readme.txt', 'safe\n')
        self.commit()

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], stderr=subprocess.STDOUT).decode().strip()

    def write(self, name, data):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(data)

    def commit(self):
        self.git('add', '-A')
        self.git('commit', '-qm', 'fixture')

    def check(self):
        return subprocess.run(['python3', str(ROOT / 'scripts/privacy-check.py'), '--repo', str(self.root)],
                              capture_output=True, text=True)

    def test_clean_and_missing_scanner(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        with unittest.mock.patch.dict(os.environ, {'PATH': '/nonexistent'}):
            with self.assertRaisesRegex(ValueError, 'required'):
                source.privacy.scan_secrets(self.root, self.root / 'config')

    def test_nested_configuration_database_key_and_archives(self):
        for name in ['src/.env', 'nested/.env.production', 'src/.npmrc', 'src-tauri/media.db',
                     'private.key', 'nested/id_ed25519', 'backup.zip', 'nested/media.sqlite-wal']:
            with self.subTest(name=name):
                self.write(name, 'synthetic local data')
                self.assertEqual(self.git('check-ignore', name), name)
                self.git('add', '-f', name)
                result = self.check()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('Forbidden tracked filename', result.stderr)
                self.git('reset', '-q', 'HEAD', '--', name)
                (self.root / name).unlink()

    def test_full_historical_secret_on_other_branch(self):
        branch = self.git('branch', '--show-current')
        self.git('checkout', '-qb', 'historical')
        # Construct a synthetic token at runtime, so test code itself contains no secret.
        self.write('credential.txt', 'token = "' + 'ghp_' + 'A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8' + '"\n')
        self.commit()
        self.write('credential.txt', 'removed\n')
        self.commit()
        self.git('checkout', '-q', branch)
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('leaks found', result.stderr)

    def test_staged_secret_even_when_working_file_is_clean(self):
        self.write('readme.txt', 'token = "' + 'ghp_' + 'A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8' + '"\n')
        self.git('add', 'readme.txt')
        self.write('readme.txt', 'safe\n')
        self.assertNotEqual(self.check().returncode, 0)

    def test_private_path_in_deleted_history_and_metadata(self):
        private = '/home/' + 'synthetic-person/Documents/project'
        self.write('readme.txt', private)
        self.commit()
        self.write('readme.txt', 'safe')
        self.commit()
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Private home path in object', result.stderr)

    def test_bare_home_paths(self):
        for path in [b'/home/' + b'synthetic-person', b'C:/Users/' + b'synthetic-person']:
            with self.subTest(path=path):
                self.assertTrue(source.privacy.private_paths(path))

    def test_commit_message(self):
        self.git('commit', '--allow-empty', '-qm', '/home/' + 'synthetic-person/project')
        self.assertNotEqual(self.check().returncode, 0)

    def test_shallow_clone_refused(self):
        clone = Path(self.temp.name) / 'shallow'
        subprocess.run(['git', 'clone', '-q', '--depth=1', self.root.as_uri(), str(clone)], check=True)
        with self.assertRaisesRegex(ValueError, 'Full history'):
            source.privacy.check(clone)

    def test_source_uses_commit_and_ignores_local_inputs(self):
        import tarfile
        for name in ['.env', 'src/.env.local', 'media.db', 'tsconfig.tsbuildinfo']:
            self.write(name, 'must remain local')
        destination = Path(self.temp.name) / 'context'
        commit = self.git('rev-parse', 'HEAD')
        source.prepare(self.root, destination, commit)
        with tarfile.open(destination / '.release-source/application-source.tar.gz') as archive:
            self.assertEqual(set(archive.getnames()), {'.gitignore', 'readme.txt'})
            for member in archive:
                self.assertEqual((member.uid, member.gid), (0, 0))
                self.assertEqual(archive.extractfile(member).read(), (self.root / member.name).read_bytes())
        self.assertEqual((destination / '.release-source/source-commit.txt').read_text().strip(), commit)
        self.write('readme.txt', 'dirty')
        with self.assertRaisesRegex(ValueError, 'clean checkout'):
            source.prepare(self.root, Path(self.temp.name) / 'dirty', commit)
        self.commit()
        with self.assertRaisesRegex(ValueError, 'requested commit'):
            source.prepare(self.root, Path(self.temp.name) / 'old', commit)

    def test_archive_attributes_cannot_change_sources(self):
        self.write('.gitattributes', 'readme.txt export-ignore\n')
        self.commit()
        with self.assertRaisesRegex(ValueError, 'file set'):
            source.prepare(self.root, Path(self.temp.name) / 'context', 'HEAD')

    def test_source_info_refuses_unprepared_directory(self):
        result = subprocess.run(['bash', str(ROOT / 'packaging/linux/source-info.sh'), self.temp.name],
                                cwd=self.root, capture_output=True)
        self.assertNotEqual(result.returncode, 0)


if __name__ == '__main__':
    unittest.main()
