# Verification

Started: 2026-09-17T19:41:16Z

Review status: complete

## Candidate ledger

No distinct candidates were submitted by `/root/architecture`, `/root/backend`, or coordinator `/root`. Canonical IDs, aliases, originator-to-confirmer mappings, and verdict sections therefore have zero entries. Confirmed: 0. Rejected: 0. Unresolved: 0. No candidates await independent confirmation. Zero candidates does not establish correctness outside the recorded coverage.

Specialist evidence is recorded in [architecture](architecture.md) and [backend](backend.md). Assignment identities and coverage are in [scope](scope.md).

## Executed checks

All commands below ran from the repository root unless stated otherwise. `REVIEW_AUDIT_DIR` names the external temporary audit directory; it is not a repository path. The snapshot, probe script, and preservation results remain there for this session. No raw logs or private paths are included here.

| Check | Command | Exit and result |
| --- | --- | --- |
| Start baseline | `git rev-parse HEAD`; `git status --porcelain=v1`; `git diff --name-status`; `git diff --cached --name-status`; `git ls-files --others --exclude-standard`; `git diff 32511af -- AGENTS.md` | All exited 0. HEAD/base is `32511af81e37b1f6ba9c099086050447303a2a44`; one unstaged routing edit and eight untracked skill files; no staged changes. |
| Invocation creation | `python3 .agents/skills/review-team/scripts/init_review.py review-team-skill` | Exit 0. Created this new UTC-named folder with six partial reports. |
| Coordinator runtime probe | `python3 "$REVIEW_AUDIT_DIR/probe_initializer.py"` | Exit 0. Ten check groups passed, including 48 concurrent creations. Full rerunnable script below. |
| Backend runtime probe | `PYTHONDONTWRITEBYTECODE=1 python3 -` with a temporary synthetic heredoc harness | Exit 0. Independent coverage of 24 concurrent calls and initializer behavior listed in backend report. This inline worker harness was not retained; coordinator reproduction is included below. |
| Tracked diff whitespace | `git diff --check 32511af81e37b1f6ba9c099086050447303a2a44 -- AGENTS.md .agents/skills/review-team` | Exit 0. Covers tracked routing only, since skill files are untracked. |
| Source references | Read-only Python Markdown target/heading assertions through `python3 -` | Exit 0. All ten skill Markdown links resolve, routing target exists, and `docs/development/testing.md#choosing-the-test-level` exists. Coordinator repeated the local-link and test-anchor checks independently. |
| Source preservation | Read-only Python SHA-256 comparison against the external start snapshot through `python3 -` | Exit 0. All 559 baseline files unchanged; only six new invocation report files. HEAD and staged/unstaged path status unchanged. |
| Report integrity | Read-only Python file/link/anchor/status checks through `python3 -` | Exit 0. All six required reports, relative targets, line references, local heading anchors, complete statuses, and README links checked. |

The initializer probe directly executes the current source. UTC clock injection fixes the timestamp for collision checks; the separate real CLI probe checks UTC with a non-UTC process timezone. Tests establish ordinary-parent name containment, not containment through symlinked ancestors.

## Workflow evidence and limits

The [architecture flow table](architecture.md#workflow-coverage) covers unavailable delegation, interruption, duplicate candidates, contradictory evidence, unresolved allegations, newly discovered confirmer findings, completion, and routing. Its basis is code-and-contract analysis. Actual faulted agent orchestration and confirmation-agent behavior were not exercised because this review produced no candidates.

The fourth-file failure test establishes that the initializer propagates an I/O failure and leaves earlier skeletons marked partial. It does not establish atomic six-file creation or recovery from actual disk exhaustion. Hostile symlink races, Windows, application tests, desktop startup, and desktop E2E remain outside executed coverage.

## Initializer probe

To repeat the coordinator probe, save the following block as `probe_initializer.py` in an external temporary directory, set `REVIEW_AUDIT_DIR` to that directory, and run `python3 "$REVIEW_AUDIT_DIR/probe_initializer.py"` from the repository root. It uses only Python's standard library, reads the initializer, creates temporary synthetic directories, and removes its fixtures on exit.

```python
import concurrent.futures
from datetime import datetime, timezone
import hashlib
import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from unittest.mock import patch

sys.dont_write_bytecode = True
SOURCE = Path.cwd() / '.agents/skills/review-team/scripts/init_review.py'
spec = importlib.util.spec_from_file_location('review_initializer', SOURCE)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
EXPECTED = {f'{name}.md' for name in ['README', 'scope', 'frontend', 'backend', 'architecture', 'verification']}

class FixedDateTime:
    @staticmethod
    def now(tz):
        assert tz is timezone.utc
        return datetime(2026, 1, 2, 3, 4, 5, tzinfo=timezone.utc)

def digest_tree(root):
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in root.rglob('*') if p.is_file()}

with tempfile.TemporaryDirectory(prefix='review-init-fixture-') as temp:
    root = Path(temp)
    with patch.object(module, 'datetime', FixedDateTime):
        folder = module.create_review(root / 'normal', ' Review TEAM / v2! ')
        assert folder.name == '2026-01-02-030405-review-team-v2'
        assert {p.name for p in folder.iterdir()} == EXPECTED
        for p in folder.iterdir():
            content = p.read_text()
            assert 'Started: 2026-01-02T03:04:05Z' in content
            assert 'Review status: partial' in content
            assert 'No review or verification results recorded yet.' in content
        print('PASS six initial reports, UTC naming, partial state, slug normalization')

        for name in ['', '   ', '!!!', '../../', '日本語']:
            try:
                module.create_review(root / 'invalid', name)
            except ValueError:
                pass
            else:
                raise AssertionError('invalid name accepted')
        assert not (root / 'invalid').exists()
        print('PASS invalid names rejected before filesystem mutation')

        for name in ['../../escape', '/absolute/path', 'a\\b', 'a' * 200, 'a' * 79 + '--x']:
            p = module.create_review(root / 'containment', name)
            assert p.resolve().parent == (root / 'containment/docs/review').resolve()
            assert len(p.name.removeprefix('2026-01-02-030405-')) <= 80
            assert re.fullmatch('[0-9a-z-]+', p.name)
        print('PASS name-based path containment and slug length cap')

        before = digest_tree(root / 'normal')
        second = module.create_review(root / 'normal', 'Review TEAM / v2!')
        third = module.create_review(root / 'normal', 'Review TEAM / v2!')
        assert second.name == folder.name + '-2'
        assert third.name == folder.name + '-3'
        after = digest_tree(root / 'normal')
        assert all(after[k] == v for k, v in before.items())
        print('PASS collision suffixes and preservation of existing reports')

        file_root = root / 'file-collision'
        parent = file_root / 'docs/review'
        parent.mkdir(parents=True)
        (parent / '2026-01-02-030405-name').write_text('sentinel')
        result = module.create_review(file_root, 'name')
        assert result.name == '2026-01-02-030405-name-2'
        assert (parent / '2026-01-02-030405-name').read_text() == 'sentinel'
        print('PASS existing file collision is preserved')

        concurrent_root = root / 'concurrent'
        with concurrent.futures.ThreadPoolExecutor(max_workers=12) as pool:
            results = list(pool.map(lambda _: module.create_review(concurrent_root, 'parallel'), range(48)))
        assert len(set(results)) == 48
        assert {p.name for p in results} == {'2026-01-02-030405-parallel'} | {
            f'2026-01-02-030405-parallel-{i}' for i in range(2,49)}
        assert all({p.name for p in folder.iterdir()} == EXPECTED for folder in results)
        print('PASS 48 concurrent creations, unique folders, six reports each')

    cli_root = root / 'cli'
    cli_root.mkdir()
    env = dict(os.environ, TZ='Pacific/Honolulu', PYTHONDONTWRITEBYTECODE='1')
    before_utc = datetime.now(timezone.utc)
    cli = subprocess.run([sys.executable, str(SOURCE), 'CLI name'], cwd=cli_root,
                         env=env, text=True, capture_output=True)
    after_utc = datetime.now(timezone.utc)
    assert cli.returncode == 0, cli.stderr
    output = Path(cli.stdout.strip())
    assert not output.is_absolute()
    stamp = datetime.strptime(output.name[:17], '%Y-%m-%d-%H%M%S').replace(tzinfo=timezone.utc)
    assert before_utc.replace(microsecond=0) <= stamp <= after_utc
    assert {p.name for p in (cli_root / output).iterdir()} == EXPECTED
    print('PASS real CLI relative output and UTC under non-UTC TZ')
    before = digest_tree(cli_root)
    for args in [[], ['!!!']]:
        cli = subprocess.run([sys.executable, str(SOURCE), *args], cwd=cli_root,
                             env=env, text=True, capture_output=True)
        assert cli.returncode == 2
    assert digest_tree(cli_root) == before
    print('PASS CLI missing/invalid name exit 2 and preserve existing files')

    obstruction_root = root / 'obstruction'
    obstruction_root.mkdir()
    (obstruction_root / 'docs').write_text('sentinel')
    cli = subprocess.run([sys.executable, str(SOURCE), 'name'], cwd=obstruction_root,
                         env=env, text=True, capture_output=True)
    assert cli.returncode != 0 and not cli.stdout
    assert (obstruction_root / 'docs').read_text() == 'sentinel'
    print('PASS obstructed parent fails without success output or overwritten data')

    original_write = Path.write_text
    def fail_fourth(path, *args, **kwargs):
        if path.name == 'backend.md':
            raise OSError('synthetic write failure')
        return original_write(path, *args, **kwargs)
    with patch.object(Path, 'write_text', fail_fourth):
        try:
            module.create_review(root / 'write-failure', 'name')
        except OSError as error:
            assert str(error) == 'synthetic write failure'
        else:
            raise AssertionError('write failure incorrectly returned success')
    leftovers = list((root / 'write-failure/docs/review').glob('*/*.md'))
    assert {p.name for p in leftovers} == {'README.md', 'scope.md', 'frontend.md'}
    assert all('Review status: partial' in p.read_text() for p in leftovers)
    print('PASS write failure propagates; three incomplete reports remain partial')

print('All coordinator initializer probes passed.')
```
