#!/usr/bin/env python3
"""Initialize an ignored deep-review record (schema 2) from the repository root.

Changes mode (default) reuses review-changes/scripts/collect_scope.py, so the scope is
merge-base(main)..HEAD plus staged, unstaged and untracked work, or --base/--range.
Paths mode (--path) reviews every tracked or untracked-but-not-ignored file under the paths.
"""

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "review-changes" / "scripts"))
from collect_scope import ScopeError, collect  # noqa: E402

SCHEMA_VERSION = 2


def git(root, *args, required=True):
    result = subprocess.run(["git", "-C", str(root), *args], capture_output=True)
    if required and result.returncode:
        raise ValueError(result.stderr.decode(errors="replace").strip())
    return result.stdout.decode("utf-8", errors="surrogateescape")


def paths(root, *args):
    return [p for p in git(root, *args).split("\0") if p]


def fingerprint(path):
    if path.is_symlink():
        return "symlink:" + hashlib.sha256(str(path.readlink()).encode()).hexdigest()
    if not path.exists():
        return None
    if not path.is_file():
        raise ValueError(f"Not a source file: {path}")
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot(root, names):
    """Hash repository-relative files, including deleted files and symlink identities."""
    result = {}
    for name in sorted(set(names)):
        path = Path(name)
        if path.is_absolute() or ".." in path.parts:
            raise ValueError(f"Source path must be repository-relative: {name}")
        if not (root / path).parent.resolve().is_relative_to(root.resolve()):
            raise ValueError(f"Source parent escapes repository: {name}")
        result[name] = fingerprint(root / path)
    return result


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=True) + "\n")


def create_review(root, name="current changes", targets=None, now=None, base=None, commit_range=None):
    root = root.resolve()
    if git(root, "rev-parse", "--show-toplevel").strip() != str(root):
        raise ValueError("Run from the repository root")
    if targets and (base or commit_range):
        raise ValueError("--path reviews whole files; do not combine it with --base or --range")
    slug = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")[:80].rstrip("-")
    if not slug:
        raise ValueError("Scope needs ASCII letters or digits")
    if (root / "raports").is_symlink():
        raise ValueError("raports must not be a symlink")
    # Check a nested sentinel as well as generated files during final validation.
    probe = "raports/.deep-review-ignore-probe/nested/evidence.json"
    if not git(root, "check-ignore", probe, required=False).strip():
        raise ValueError("Add /raports/ to the repository .gitignore first")

    head = git(root, "rev-parse", "--verify", "HEAD", required=False).strip() or None
    staged = paths(root, "diff", "--cached", "--name-only", "-z", "--no-renames")
    unstaged = paths(root, "diff", "--name-only", "-z", "--no-renames")
    untracked = paths(root, "ls-files", "--others", "--exclude-standard", "-z")
    limitations = [] if head else ["HEAD unavailable"]
    committed = []

    if targets:
        for target in targets:
            if Path(target).is_absolute() or ".." in Path(target).parts:
                raise ValueError("Targets must be repository-relative paths")
        selected = paths(root, "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", *targets)
        comparison_base = head
    elif head:
        try:
            scope = collect(root, [commit_range] if commit_range else [], base=base)
        except ScopeError as error:
            raise ValueError(str(error)) from error
        selected = [f["path"] for f in scope["files"]]
        # Old paths of renames are part of the change: their hash is null (removed).
        selected += [f["old_path"] for f in scope["files"] if f["old_path"]]
        comparison_base = scope["base"]
        if commit_range:
            staged, unstaged, untracked = [], [], []
            end = scope["target"]
        else:
            end = "HEAD"
        if comparison_base and comparison_base != end:
            committed = paths(root, "diff", "--name-only", "-z", "--no-renames", comparison_base, end)
        limitations += scope["notes"]
    else:
        selected = sorted(set(staged + unstaged + untracked))
        comparison_base = None

    hashes = snapshot(root, selected)
    no_changes = not targets and not selected and head is not None
    started = now or datetime.now(timezone.utc)
    parent = root / "raports"
    parent.mkdir(exist_ok=True)
    stem = f"{started:%Y%m%dT%H%M%SZ}-{slug}"
    number = 1
    while True:
        folder = parent / (stem if number == 1 else f"{stem}-{number}")
        try:
            folder.mkdir()
            break
        except FileExistsError:
            number += 1
    (folder / "agents").mkdir()
    (folder / "verification").mkdir()
    write_json(folder / "run.json", {
        "schema_version": SCHEMA_VERSION, "started_at": started.isoformat(), "status": "partial",
        "scope": {"request": name, "mode": "paths" if targets else "changes",
                  "targets": targets or ([commit_range] if commit_range else []),
                  "included": sorted(hashes), "supporting": [], "exclusions": [],
                  "no_changes": no_changes},
        "baseline": {"head": head, "comparison_base": comparison_base, "committed": committed,
                     "staged": staged, "unstaged": unstaged, "untracked": untracked,
                     "hashes": hashes},
        "assignments": [], "coverage": [], "handoffs": [], "runtime_owner": None,
        "source_changes": [], "limitations": limitations or ["Investigation pending"],
    })
    write_json(folder / "findings.json", {"candidates": [], "duplicates": []})
    (folder / "context.md").write_text("# Context\n\nPending dependency map and applicable contracts.\n")
    (folder / "report.md").write_text(f"# {name}\n\nStatus: partial. Investigation pending.\n")
    return folder


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("name", nargs="?", default="current changes")
    parser.add_argument("--path", action="append", dest="targets", help="review whole files under this path")
    parser.add_argument("--base", help="compare the working tree with this revision")
    parser.add_argument("--range", dest="commit_range", help="review a committed range A..B or A...B only")
    args = parser.parse_args()
    try:
        folder = create_review(Path.cwd(), args.name, args.targets, base=args.base, commit_range=args.commit_range)
        print(folder.relative_to(Path.cwd()))
    except (ValueError, OSError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
