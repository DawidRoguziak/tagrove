#!/usr/bin/env python3
"""Describe the change set to review as JSON. Read-only: it only runs git queries.

Default scope: merge-base(main)..HEAD plus staged, unstaged and untracked work.
On the main branch (or with no main branch) the base is HEAD, so only uncommitted
work is in scope. Arguments:

  A..B / A...B     review a committed range only (A...B diffs from the merge base)
  REV              same as --base REV when REV is not an existing path
  --base REV       compare the working tree with REV instead of merge-base(main)
  --staged         review only the index against HEAD
  PATH ...         keep only changed files under these repository-relative paths
"""

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

MAIN_CANDIDATES = ("main", "origin/main", "master", "origin/master")

# Files that are only scanned for churn, never reviewed line by line.
LOCKFILES = {"bun.lock", "src-tauri/Cargo.lock", "package-lock.json", "yarn.lock"}

# Paths where cost scales with library size (up to 1M results). Changes here
# always get the performance lens.
HOT_PATHS = (
    "src/library/**",
    "src/features/gallery/**",
    "src/app/AppGalleryView.tsx",
    "src/ui/controls/ThumbnailImage.tsx",
    "src/features/search/**",
    "src/**/*worker*",
    "src-tauri/crates/media_tagger_core/src/db.rs",
    "src-tauri/crates/media_tagger_core/src/db/**",
    "src-tauri/crates/media_tagger_core/src/services/asset_query_service.rs",
    "src-tauri/crates/media_tagger_core/src/services/db_pool.rs",
    "src-tauri/crates/media_tagger_core/src/services/thumb_*.rs",
    "src-tauri/crates/media_tagger_core/src/services/scan_service.rs",
    "src-tauri/crates/media_tagger_core/src/thumbs.rs",
    "src-tauri/crates/media_tagger_core/src/indexer.rs",
    "src-tauri/src/adapter/thumb_scheme.rs",
)

# A change larger than this (in Rust/frontend source lines) also gets the
# performance lens, because big diffs rarely leave hot paths untouched.
PERFORMANCE_LINE_THRESHOLD = 300

# Docs/config-only changes up to this size are reviewed by the coordinator alone.
SOLO_LINE_THRESHOLD = 50


class ScopeError(Exception):
    pass


def git(root, *args, check=True):
    result = subprocess.run(["git", "-C", str(root), *args], capture_output=True)
    if check and result.returncode:
        raise ScopeError(result.stderr.decode(errors="replace").strip() or f"git {' '.join(args)} failed")
    return result.stdout.decode("utf-8", errors="surrogateescape")


def rev(root, name):
    out = git(root, "rev-parse", "--verify", "--quiet", "--end-of-options", f"{name}^{{commit}}", check=False)
    return out.strip() or None


def glob_regex(pattern):
    """Translate a rules-style glob (`**`, `*`, `?`) into an anchored regex."""
    out, i = [], 0
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out.append("(?:.*/)?")
            i += 3
        elif pattern.startswith("**", i):
            out.append(".*")
            i += 2
        elif pattern[i] == "*":
            out.append("[^/]*")
            i += 1
        elif pattern[i] == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(pattern[i]))
            i += 1
    return re.compile("".join(out) + r"\Z")


def matches(path, patterns):
    return any(glob_regex(p).match(path) for p in patterns)


def category(path):
    if path in LOCKFILES:
        return "lockfile"
    if path.startswith("src/i18n/locales/"):
        return "locales"
    if path.startswith("e2e/"):
        return "e2e"
    if path.startswith("src-tauri/tests/") or "/__tests__/" in path or re.search(r"\.(test|spec)\.[tj]sx?$", path):
        return "tests"
    if re.match(r"src-gpui/crates/[^/]+/tests/", path):
        return "tests"
    if path.startswith(("src-tauri/", "src-gpui/")) and path.endswith(".rs"):
        return "rust"
    if path.startswith("src/") or path == "index.html":
        return "frontend"
    if path.endswith(".md") or path.startswith("docs/"):
        return "docs"
    if path.startswith(("scripts/", "packaging/", ".claude/", ".github/")) or path.endswith((".py", ".sh")):
        return "tooling"
    return "config"


def load_rules(root):
    """Return path-scoped rules with their globs and the doc pages they point to."""
    rules = []
    for file in sorted((root / ".claude/rules").glob("*.md")):
        text = file.read_text(encoding="utf-8")
        front = re.match(r"---\n(.*?)\n---\n", text, re.S)
        globs = re.findall(r'^\s*-\s*"?([^"\n]+?)"?\s*$', front.group(1), re.M) if front else []
        docs = sorted(set(re.findall(r"docs/[\w./-]+\.md", text)))
        rules.append({"rule": file.relative_to(root).as_posix(), "globs": globs, "docs": docs})
    return rules


def parse_name_status(raw):
    """Parse `git diff --name-status -z -M` into {path: (status, old_path)}."""
    items, parts, i = {}, raw.split("\0"), 0
    while i < len(parts) and parts[i]:
        code = parts[i]
        if code[0] in "RC":
            items[parts[i + 2]] = (code[0], parts[i + 1])
            i += 3
        else:
            items[parts[i + 1]] = (code[0], None)
            i += 2
    return items


def parse_numstat(raw):
    """Parse `git diff --numstat -z -M` into {path: (added, removed)}; binary is None."""
    stats, parts, i = {}, raw.split("\0"), 0
    while i < len(parts) and parts[i]:
        added, removed, path = parts[i].split("\t", 2)
        if path == "":  # rename: old and new path follow as separate fields
            path = parts[i + 2]
            i += 3
        else:
            i += 1
        stats[path] = None if added == "-" else (int(added), int(removed))
    return stats


def line_count(path):
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\0" in data[:8192]:
        return None
    return data.count(b"\n") + (1 if data and not data.endswith(b"\n") else 0)


def main_branch(root):
    for name in MAIN_CANDIDATES:
        if rev(root, name):
            return name
    return None


def collect(root, targets=(), base=None, staged=False):
    root = Path(git(root, "rev-parse", "--show-toplevel").strip())
    head = rev(root, "HEAD")
    branch = git(root, "branch", "--show-current", check=False).strip() or None
    ranges = [t for t in targets if ".." in t and not (root / t).exists()]
    paths = [t for t in targets if t not in ranges]
    # A bare revision that is not a path means "compare the working tree with it".
    revisions = [t for t in paths if not (root / t).exists() and rev(root, t)]
    if revisions:
        if len(revisions) > 1 or base:
            raise ScopeError("Give at most one base revision")
        base = revisions[0]
        paths = [t for t in paths if t != base]
    if len(ranges) > 1 or (ranges and (base or staged)):
        raise ScopeError("Give one commit range, or --base/--staged, not both")
    for p in paths:
        if Path(p).is_absolute() or ".." in Path(p).parts:
            raise ScopeError(f"Paths must be repository-relative: {p}")

    notes, untracked = [], []
    if ranges:
        spec = ranges[0]
        three_dot = "..." in spec
        left, right = spec.split("..." if three_dot else "..", 1)
        left_sha, right_sha = rev(root, left or "HEAD"), rev(root, right or "HEAD")
        if not left_sha or not right_sha:
            raise ScopeError(f"Unknown revision in range: {spec}")
        if three_dot:
            left_sha = git(root, "merge-base", left_sha, right_sha).strip()
        mode, base_sha, target = "range", left_sha, right_sha
        diff_args = [base_sha, target]
        diff_command = f"git diff -M {base_sha[:12]} {target[:12]}"
    elif staged:
        if not head:
            raise ScopeError("--staged needs an existing HEAD commit")
        mode, base_sha, target = "staged", head, "index"
        diff_args = ["--cached", head]
        diff_command = "git diff -M --cached HEAD"
    else:
        mode, target = "worktree", "worktree"
        if base:
            base_sha = rev(root, base)
            if not base_sha:
                raise ScopeError(f"Unknown base revision: {base}")
        else:
            main = main_branch(root)
            base_sha = head
            if main and head and branch not in (main, main.split("/")[-1]):
                base_sha = git(root, "merge-base", main, "HEAD", check=False).strip() or head
                if base_sha == head:
                    notes.append(f"HEAD is already contained in {main}; only uncommitted work is in scope")
            elif not main:
                notes.append("No main branch found; base is HEAD")
        if not base_sha:
            raise ScopeError("No HEAD commit yet; commit once before reviewing")
        diff_args = [base_sha]
        diff_command = f"git diff -M {base_sha[:12]}"
        untracked = [p for p in git(root, "ls-files", "--others", "--exclude-standard", "-z").split("\0") if p]
        if untracked:
            diff_command += "  # plus untracked files listed with status 'A?'"

    status = parse_name_status(git(root, "diff", "--name-status", "-z", "-M", *diff_args, "--"))
    numstat = parse_numstat(git(root, "diff", "--numstat", "-z", "-M", *diff_args, "--"))
    for path in untracked:
        status[path] = ("A?", None)
        lines = line_count(root / path)
        numstat[path] = None if lines is None else (lines, 0)

    if paths:
        prefixes = [p.rstrip("/") for p in paths]
        status = {k: v for k, v in status.items()
                  if any(k == p or k.startswith(p + "/") or (v[1] or "").startswith(p + "/") for p in prefixes)}
        diff_command += " -- " + " ".join(prefixes)

    rules = load_rules(root)
    files, matched_rules, docs = [], {}, set()
    for path in sorted(status):
        code, old = status[path]
        stat = numstat.get(path)
        cat = category(path)
        hit = [r["rule"] for r in rules if matches(path, r["globs"]) or (old and matches(old, r["globs"]))]
        for r in rules:
            if r["rule"] in hit:
                matched_rules.setdefault(r["rule"], []).append(path)
                docs.update(r["docs"])
        files.append({
            "path": path, "status": code, "old_path": old, "category": cat,
            "added": stat[0] if stat else None, "removed": stat[1] if stat else None,
            "binary": stat is None, "hot_path": matches(path, HOT_PATHS) or bool(old and matches(old, HOT_PATHS)),
            "rules": hit,
        })

    app_code = [f for f in files if f["category"] in ("rust", "frontend")]
    source_lines = sum((f["added"] or 0) + (f["removed"] or 0) for f in app_code)
    total_lines = sum((f["added"] or 0) + (f["removed"] or 0) for f in files)
    cats = {f["category"] for f in files}
    lenses = []
    if files:
        solo = cats <= {"docs", "config"} and total_lines <= SOLO_LINE_THRESHOLD and \
            not any(f["rules"] for f in files) and "lockfile" not in cats
        if not solo:
            lenses.append("review-conventions")
            if any(f["path"].startswith(("src-tauri/", "src-gpui/")) for f in files):
                lenses.append("review-rust")
            if any(f["path"].startswith("src-gpui/") for f in files):
                lenses.append("review-gpui")
            if cats & {"frontend", "locales", "e2e"} or any(
                    f["path"].startswith("src/") for f in files):
                lenses.append("review-frontend")
            if any(f["hot_path"] for f in files) or source_lines > PERFORMANCE_LINE_THRESHOLD:
                lenses.append("review-performance")

    commits = []
    if head and mode in ("worktree", "range") and base_sha and base_sha != target:
        end = target if mode == "range" else "HEAD"
        log = git(root, "log", "--format=%h%x09%s", f"{base_sha}..{end}", check=False)
        commits = [dict(zip(("sha", "subject"), line.split("\t", 1))) for line in log.splitlines() if line]

    return {
        "root": str(root), "mode": mode, "branch": branch, "head": head,
        "base": base_sha, "target": target, "path_filter": paths,
        "diff_command": diff_command, "commits": commits,
        "empty": not files, "totals": {"files": len(files), "changed_lines": total_lines,
                                       "app_source_lines": source_lines},
        "categories": sorted(cats), "suggested_lenses": lenses,
        "rules": [{"rule": r, "files": fs} for r, fs in matched_rules.items()],
        "docs": sorted(docs), "notes": notes, "files": files,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("targets", nargs="*", help="commit range (A..B, A...B) or repository-relative paths")
    parser.add_argument("--base", help="compare the working tree with this revision")
    parser.add_argument("--staged", action="store_true", help="only the index against HEAD")
    args = parser.parse_args(argv)
    try:
        result = collect(Path.cwd(), args.targets, args.base, args.staged)
    except (ScopeError, OSError) as error:
        print(f"collect_scope: {error}", file=sys.stderr)
        return 2
    json.dump(result, sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
