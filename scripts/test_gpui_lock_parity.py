import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / 'scripts/gpui-lock-parity.py'
spec = importlib.util.spec_from_file_location('gpui_lock_parity', SCRIPT)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

CRATES_IO = guard.CRATES_IO
ZED_REV = 'a' * 40
OTHER_REV = 'b' * 40


def package(name, version, dependencies=(), source=CRATES_IO, checksum=None):
    entry = {'name': name, 'version': version}
    if source:
        entry['source'] = source
        if source == CRATES_IO:
            entry['checksum'] = checksum or f'sum-{name}-{version}'
    if dependencies:
        entry['dependencies'] = list(dependencies)
    return entry


def git(url, rev, commit=None):
    return f'git+{url}?rev={rev}#{commit or rev}'


def tauri_lock():
    return [
        package('media_tagger', '0.1.1', ['media_tagger_core', 'tauri'], source=None),
        package('media_tagger_core', '0.1.1', ['rusqlite', 'rand 0.8.5', 'tempfile'], source=None),
        package('rusqlite', '0.41.1', ['libsqlite3-sys']),
        package('libsqlite3-sys', '0.39.1'),
        package('rand', '0.8.5'),
        package('rand', '0.9.2'),
        package('tempfile', '3.27.0'),
        package('tauri', '2.11.5', ['rand 0.9.2']),
    ]


def gpui_lock(**replaced):
    packages = {
        'media_tagger_gpui': package('media_tagger_gpui', '0.1.1', ['gpui', 'media_tagger_core'], source=None),
        'media_tagger_core': package('media_tagger_core', '0.1.1', ['rusqlite', 'rand 0.8.5'], source=None),
        'rusqlite': package('rusqlite', '0.41.1', ['libsqlite3-sys']),
        'libsqlite3-sys': package('libsqlite3-sys', '0.39.1'),
        'rand 0.8': package('rand', '0.8.5'),
        'rand 0.9': package('rand', '0.9.4'),
        'gpui': package('gpui', '0.2.2', ['rand 0.9.4'], source=git(guard.ZED, ZED_REV)),
    }
    packages.update(replaced)
    return [entry for entry in packages.values() if entry is not None]


def lock_text(packages):
    lines = ['version = 4', '']
    for entry in packages:
        lines.append('[[package]]')
        for field in ('name', 'version', 'source', 'checksum'):
            if field in entry:
                lines.append(f'{field} = "{entry[field]}"')
        if 'dependencies' in entry:
            lines.append('dependencies = [')
            lines.extend(f' "{reference}",' for reference in entry['dependencies'])
            lines.append(']')
        lines.append('')
    return '\n'.join(lines)


def manifest_text(rev=ZED_REV, extra=''):
    return f'''[workspace]
members = ["crates/media_tagger_gpui"]

[workspace.dependencies]
media_tagger_core = {{ path = "../src-tauri/crates/media_tagger_core" }}
serde_json = "1.0"
gpui = {{ git = "{guard.ZED}", rev = "{rev}" }}
gpui_platform = {{ git = "{guard.ZED}", rev = "{rev}" }}
{extra}'''


class CoreParityTests(unittest.TestCase):
    def test_identical_core_closures_pass(self):
        problems, notes, compared = guard.parity(tauri_lock(), gpui_lock())
        self.assertEqual(problems, [])
        self.assertEqual(compared, 3)
        self.assertEqual(notes, ['only in the core closure of src-tauri/Cargo.lock '
                                 '(feature unification or dev-dependencies): tempfile 3.27.0'])

    def test_packages_outside_the_core_closure_may_differ(self):
        # rand 0.9 is 0.9.2 for Tauri and 0.9.4 for GPUI, and the core uses neither.
        problems, _, _ = guard.parity(tauri_lock(), gpui_lock())
        self.assertEqual(problems, [])

    def test_a_drifted_transitive_core_package_fails(self):
        drifted = gpui_lock(**{'libsqlite3-sys': package('libsqlite3-sys', '0.39.2')})
        problems, _, _ = guard.parity(tauri_lock(), drifted)
        self.assertEqual(len(problems), 1)
        self.assertIn('libsqlite3-sys: src-tauri/Cargo.lock has 0.39.1', problems[0])
        self.assertIn('src-gpui/Cargo.lock has 0.39.2', problems[0])

    def test_a_drifted_direct_core_package_fails(self):
        drifted = gpui_lock(**{'rand 0.8': package('rand', '0.8.6'),
                               'media_tagger_core': package('media_tagger_core', '0.1.1',
                                                            ['rusqlite', 'rand 0.8.6'], source=None)})
        problems, _, _ = guard.parity(tauri_lock(), drifted)
        self.assertEqual(len(problems), 1)
        self.assertTrue(problems[0].startswith('rand: '), problems)

    def test_same_version_with_another_checksum_or_source_fails(self):
        for changed in (package('rusqlite', '0.41.1', ['libsqlite3-sys'], checksum='other'),
                        package('rusqlite', '0.41.1', ['libsqlite3-sys'],
                                source=git('https://example.invalid/rusqlite', OTHER_REV))):
            with self.subTest(source=changed.get('source')):
                problems, _, _ = guard.parity(tauri_lock(), gpui_lock(rusqlite=changed))
                self.assertEqual(len(problems), 1)
                self.assertTrue(problems[0].startswith('rusqlite: '), problems)

    def test_a_package_only_one_closure_contains_is_a_note(self):
        unified = gpui_lock(rusqlite=package('rusqlite', '0.41.1', ['libsqlite3-sys', 'hashlink']))
        unified.append(package('hashlink', '0.11.0'))
        problems, notes, _ = guard.parity(tauri_lock(), unified)
        self.assertEqual(problems, [])
        self.assertTrue(any('src-gpui/Cargo.lock' in note and 'hashlink 0.11.0' in note for note in notes), notes)

    def test_a_lock_without_the_core_is_an_error(self):
        without_core = [entry for entry in gpui_lock() if entry['name'] != 'media_tagger_core']
        with self.assertRaisesRegex(ValueError, 'src-gpui/Cargo.lock: expected media_tagger_core once'):
            guard.parity(tauri_lock(), without_core)
        with self.assertRaisesRegex(ValueError, 'src-tauri/Cargo.lock: expected media_tagger_core once'):
            guard.parity([entry for entry in tauri_lock() if entry['name'] != 'media_tagger_core'], gpui_lock())

    def test_compatible_lines(self):
        self.assertEqual(guard.compat_line('1.4.2'), '1')
        self.assertEqual(guard.compat_line('0.41.1'), '0.41')
        self.assertEqual(guard.compat_line('0.0.3'), '0.0.3')
        self.assertEqual(guard.compat_line('1.0.4+wasi-0.2.12'), '1')
        self.assertEqual(guard.compat_line('0.4.0-zed'), '0.4')


class SourceTests(unittest.TestCase):
    def test_crates_io_zed_and_repositories_pinned_by_zed_pass(self):
        packages = gpui_lock()
        for url, rev in guard.PINNED_BY_ZED.items():
            packages.append(package(f'pinned-{len(packages)}', '1.0.0', source=git(url, rev)))
        self.assertEqual(guard.source_problems(packages, ZED_REV), [])

    def test_another_zed_commit_fails(self):
        packages = gpui_lock(gpui=package('gpui', '0.2.2', ['rand 0.9.4'], source=git(guard.ZED, OTHER_REV)))
        problems = guard.source_problems(packages, ZED_REV)
        self.assertEqual(len(problems), 1)
        self.assertIn(f'expected {ZED_REV}', problems[0])

    def test_an_unreviewed_git_repository_or_registry_fails(self):
        for source, expected in (
                (git('https://github.com/someone/gpui-fork', OTHER_REV), 'neither Zed nor pinned by Zed'),
                (git(next(iter(guard.PINNED_BY_ZED)), OTHER_REV), 'expected '),
                ('git+https://github.com/zed-industries/zed?branch=main#' + ZED_REV, 'unsupported source'),
                ('registry+https://example.invalid/index', 'unsupported source')):
            with self.subTest(source=source):
                problems = guard.source_problems(gpui_lock(extra=package('extra', '1.0.0', source=source)), ZED_REV)
                self.assertEqual(len(problems), 1)
                self.assertIn(expected, problems[0])

    def test_third_party_gpui_packages_are_refused_even_from_crates_io(self):
        for name in ('gpui-component', 'gpui-pre'):
            with self.subTest(name=name):
                problems = guard.source_problems(gpui_lock(extra=package(name, '0.5.0')), ZED_REV)
                self.assertEqual(len(problems), 1)
                self.assertIn('refused', problems[0])

    def test_manifest_pins_one_full_zed_rev(self):
        with tempfile.TemporaryDirectory(prefix='tagrove-gpui-lock-test-') as temporary:
            manifest = Path(temporary) / 'Cargo.toml'
            manifest.write_text(manifest_text())
            self.assertEqual(guard.pinned_zed_rev(manifest), ZED_REV)
            for text, expected in (
                    (manifest_text(extra=f'sum_tree = {{ git = "{guard.ZED}", rev = "{OTHER_REV}" }}\n'),
                     'expected one pinned Zed rev'),
                    (manifest_text(rev='d01b134'), 'full 40-character rev'),
                    (manifest_text(extra=f'sum_tree = {{ git = "{guard.ZED}", branch = "main" }}\n'),
                     'full 40-character rev'),
                    (manifest_text(extra='other = { git = "https://github.com/someone/other", rev = "'
                                         + OTHER_REV + '" }\n'), 'only ' + guard.ZED),
                    ('[workspace]\nmembers = []\n', 'expected one pinned Zed rev')):
                with self.subTest(expected=expected, text=text[-80:]):
                    manifest.write_text(text)
                    with self.assertRaisesRegex(ValueError, expected):
                        guard.pinned_zed_rev(manifest)


class CommandTests(unittest.TestCase):
    def repository(self, tauri, gpui, manifest=None):
        temporary = tempfile.TemporaryDirectory(prefix='tagrove-gpui-lock-test-')
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / 'src-tauri').mkdir()
        (root / 'src-gpui').mkdir()
        (root / 'src-tauri/Cargo.lock').write_text(lock_text(tauri))
        (root / 'src-gpui/Cargo.lock').write_text(lock_text(gpui))
        (root / 'src-gpui/Cargo.toml').write_text(manifest or manifest_text())
        return root

    def run_guard(self, root):
        return subprocess.run(['python3', str(SCRIPT), '--repo', str(root)], capture_output=True, text=True)

    def test_command_passes_on_matching_locks(self):
        result = self.run_guard(self.repository(tauri_lock(), gpui_lock()))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('3 core packages identical in both locks', result.stdout)

    def test_command_fails_after_a_core_package_drifts(self):
        drifted = gpui_lock(**{'libsqlite3-sys': package('libsqlite3-sys', '0.39.2')})
        result = self.run_guard(self.repository(tauri_lock(), drifted))
        self.assertEqual(result.returncode, 1)
        self.assertIn('libsqlite3-sys', result.stderr)
        self.assertIn('--precise', result.stderr)

    def test_command_fails_on_an_unreviewed_source_and_on_a_missing_lock(self):
        foreign = gpui_lock(extra=package('extra', '1.0.0', source=git('https://github.com/someone/extra', OTHER_REV)))
        result = self.run_guard(self.repository(tauri_lock(), foreign))
        self.assertEqual(result.returncode, 1)
        self.assertIn('neither Zed nor pinned by Zed', result.stderr)
        root = self.repository(tauri_lock(), gpui_lock())
        (root / 'src-gpui/Cargo.lock').unlink()
        result = self.run_guard(root)
        self.assertEqual(result.returncode, 1)
        self.assertIn('GPUI lock check failed', result.stderr)

    def test_the_repository_locks_pass(self):
        result = self.run_guard(ROOT)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == '__main__':
    unittest.main()
