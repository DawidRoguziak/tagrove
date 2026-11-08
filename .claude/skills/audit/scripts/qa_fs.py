#!/usr/bin/env python3
"""The only file operations QA agents may perform, confined to the active session's copy.

  copy [--block S1]                 Reflink the block's dataset folders (qa-blocks.json) into
                                    <session media>/dataset/ with `cp -a --reflink=always`.
  touch <path> (--mtime ISO | --days-ago N)
  rm <path> [--recursive]
  mv <src> <dst>
  chmod <octal> <path>
  write-edge <kind> <name>          kinds: zero-byte, corrupt-jpg, wrong-ext, png, symlink-inside
                                    (at most 50 edge files per session, below media/edge/).

Every target's realpath must lie inside session.json `media`, which must itself lie below the
run's QA TMPDIR (`<dataset>/for-claude-tests/q`). The operation then re-executes inside
bubblewrap with the whole filesystem read-only except that media directory, and re-validates
there. Each call is logged to <run>/qa/fs-log.jsonl. Symlinks are never followed for writes.
"""
from __future__ import annotations

import argparse
import datetime as dt
import fnmatch
import json
import os
import re
import shutil
import stat
import struct
import subprocess
import sys
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_lib as lib  # noqa: E402

INNER = "MEDIATAGGER_QA_FS_INNER"
EDGE_LIMIT = 50
EDGE_KINDS = ("zero-byte", "corrupt-jpg", "wrong-ext", "png", "symlink-inside")
SESSION_ID = re.compile(r"^[a-f0-9]{32}$")


class Refused(lib.AuditError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise Refused(message)


# ------------------------------------------------------------------ context


def session_context(repo: Path, run: dict) -> dict:
    """Resolve and cross-check the active session's media directory."""
    active = run.get("active_session") or {}
    session_id = active.get("id", "")
    require(SESSION_ID.match(session_id) is not None, "no READY QA session is recorded in run.json")
    manifest = lib.read_json(repo / "artifacts" / "app-control" / session_id / "session.json")
    require(manifest.get("id") == session_id and manifest.get("status") in ("ready", "tainted"),
            "the session manifest is not a live session")
    tmpdir = os.path.realpath(run["qa_tmpdir"])
    media = os.path.realpath(manifest["media"])
    scratch = os.path.realpath(manifest["scratch"])
    require(os.path.dirname(scratch) == tmpdir, "session scratch is not below the run's QA TMPDIR")
    require(os.path.basename(scratch).startswith("media-tagger-control-"), "unexpected scratch name")
    require(media == os.path.join(scratch, "media"), "session media is not <scratch>/media")
    with open(os.path.join(scratch, "owner"), encoding="utf-8") as owner:
        require(owner.read() == session_id, "scratch owner differs from the session")
    dataset = os.path.realpath(run["dataset"])
    require(lib.within(tmpdir, os.path.join(dataset, "for-claude-tests")), "QA TMPDIR is not in for-claude-tests")
    return {"session": session_id, "media": media, "dataset": dataset, "tmpdir": tmpdir}


def inside(path: str, media: str) -> str:
    """Resolve a user path (absolute, or relative to media) and require it below media."""
    candidate = path if os.path.isabs(path) else os.path.join(media, path)
    parent = os.path.realpath(os.path.dirname(os.path.abspath(candidate)))
    resolved = os.path.join(parent, os.path.basename(candidate))
    require(os.path.basename(candidate) not in ("", ".", ".."), f"refusing path {path!r}")
    require(lib.within(parent, media), f"{path!r} is outside the session copy")
    require(resolved != media, "refusing to operate on the media root itself")
    return resolved


def not_symlink(path: str) -> None:
    require(not os.path.islink(path), f"{path!r} is a symbolic link")


# ------------------------------------------------------------------ operations


def copy_sources(dataset: str, patterns: list[str]) -> list[str]:
    names = sorted(name for name in os.listdir(dataset)
                   if name != "for-claude-tests" and os.path.isdir(os.path.join(dataset, name))
                   and not os.path.islink(os.path.join(dataset, name)))
    chosen: list[str] = []
    for pattern in patterns:
        first = pattern.endswith(":first")
        glob = pattern[: -len(":first")] if first else pattern
        matches = [name for name in names if fnmatch.fnmatchcase(name, glob)]
        require(matches, f"no dataset folder matches {pattern!r}")
        for name in matches[:1] if first else matches:
            if name not in chosen:
                chosen.append(name)
    return chosen


def op_copy(ctx: dict, block: str) -> dict:
    blocks = lib.load_reference("qa-blocks.json")["blocks"]
    require(block in blocks, f"unknown block {block}")
    target_root = os.path.join(ctx["media"], "dataset")
    os.makedirs(target_root, exist_ok=True)
    copied = []
    for name in copy_sources(ctx["dataset"], blocks[block]["copy"]):
        source = os.path.join(ctx["dataset"], name)
        target = inside(os.path.join("dataset", name), ctx["media"])
        require(not os.path.lexists(target), f"{name} was already copied")
        subprocess.run(["cp", "-a", "--reflink=always", "--", source, target], check=True)
        copied.append(name)
    return {"copied": copied}


def op_touch(ctx: dict, path: str, mtime: str | None, days_ago: int | None) -> dict:
    target = inside(path, ctx["media"])
    require(os.path.exists(target), f"{path!r} does not exist")
    not_symlink(target)
    if mtime is not None:
        stamp = dt.datetime.fromisoformat(mtime)
        if stamp.tzinfo is None:
            stamp = stamp.replace(tzinfo=dt.timezone.utc)
        seconds = stamp.timestamp()
    else:
        require(days_ago is not None and 0 <= days_ago <= 36500, "--days-ago must be 0..36500")
        seconds = dt.datetime.now(dt.timezone.utc).timestamp() - days_ago * 86400
    targets = [target]
    if os.path.isdir(target):
        for root, dirs, files in os.walk(target):
            targets += [os.path.join(root, name) for name in files if not os.path.islink(os.path.join(root, name))]
    for item in targets:
        os.utime(item, (seconds, seconds), follow_symlinks=False)
    return {"touched": len(targets)}


def op_rm(ctx: dict, path: str, recursive: bool) -> dict:
    target = inside(path, ctx["media"])
    require(os.path.lexists(target), f"{path!r} does not exist")
    if os.path.islink(target) or not os.path.isdir(target):
        os.unlink(target)
        return {"removed": 1}
    require(recursive, "use --recursive to remove a directory")
    require(lib.within(target, os.path.join(ctx["media"], "dataset")) or lib.within(target, os.path.join(ctx["media"], "edge")),
            "only folders below media/dataset or media/edge may be removed recursively")
    os.chmod(target, stat.S_IMODE(os.lstat(target).st_mode) | stat.S_IRWXU)
    for root, dirs, _ in os.walk(target):
        for name in dirs:
            child = os.path.join(root, name)
            if not os.path.islink(child):
                os.chmod(child, stat.S_IMODE(os.lstat(child).st_mode) | stat.S_IRWXU)
    shutil.rmtree(target)
    return {"removed": "tree"}


def op_mv(ctx: dict, source: str, target: str) -> dict:
    src = inside(source, ctx["media"])
    dst = inside(target, ctx["media"])
    require(os.path.lexists(src), f"{source!r} does not exist")
    require(not os.path.lexists(dst), f"{target!r} already exists")
    os.rename(src, dst)
    return {"moved": True}


def op_chmod(ctx: dict, mode: str, path: str) -> dict:
    require(re.fullmatch(r"[0-7]{3}", mode) is not None, "mode must be three octal digits")
    target = inside(path, ctx["media"])
    require(os.path.exists(target), f"{path!r} does not exist")
    not_symlink(target)
    os.chmod(target, int(mode, 8))
    return {"mode": mode}


def tiny_png() -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", 4, 4, 8, 2, 0, 0, 0)
    pixels = zlib.compress(b"".join(b"\x00" + b"\x40\x80\xc0" * 4 for _ in range(4)))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", pixels) + chunk(b"IEND", b"")


def op_write_edge(ctx: dict, kind: str, name: str) -> dict:
    require(kind in EDGE_KINDS, f"kind must be one of {', '.join(EDGE_KINDS)}")
    require("/" not in name and name not in (".", "..") and 0 < len(name.encode()) <= 255, "bad file name")
    edge = os.path.join(ctx["media"], "edge")
    os.makedirs(edge, exist_ok=True)
    require(len(os.listdir(edge)) < EDGE_LIMIT, f"the session already has {EDGE_LIMIT} edge files")
    target = inside(os.path.join("edge", name), ctx["media"])
    require(not os.path.lexists(target), f"{name!r} already exists")
    if kind == "symlink-inside":
        images = sorted(p for p in os.listdir(edge) if not os.path.islink(os.path.join(edge, p)))
        require(images, "create a png edge file first; symlinks point only at edge files")
        os.symlink(images[0], target)
        return {"created": name, "points_to": images[0]}
    payload = {
        "zero-byte": b"",
        "corrupt-jpg": b"\xff\xd8\xff\xe0" + b"\x00garbage-not-a-jpeg" * 8,
        "wrong-ext": tiny_png(),
        "png": tiny_png(),
    }[kind]
    fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    with os.fdopen(fd, "wb") as handle:
        handle.write(payload)
    return {"created": name, "bytes": len(payload)}


def perform(ctx: dict, args) -> dict:
    if args.command == "copy":
        return op_copy(ctx, args.block)
    if args.command == "touch":
        return op_touch(ctx, args.path, args.mtime, args.days_ago)
    if args.command == "rm":
        return op_rm(ctx, args.path, args.recursive)
    if args.command == "mv":
        return op_mv(ctx, args.source, args.target)
    if args.command == "chmod":
        return op_chmod(ctx, args.mode, args.path)
    return op_write_edge(ctx, args.kind, args.name)


# ------------------------------------------------------------------ sandbox


def sandbox_argv(media: str, script: str, argv: list[str]) -> list[str]:
    """Everything read-only except the session media directory; no network, no new privileges."""
    return [
        "bwrap", "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp",
        "--bind", media, media, "--unshare-net", "--unshare-ipc", "--die-with-parent", "--new-session",
        "--setenv", INNER, "1", "--chdir", media,
        sys.executable, "-I", script, *argv,
    ]


def log(run_dir: Path, entry: dict) -> None:
    path = run_dir / "qa" / "fs-log.jsonl"
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(json.dumps({"time": lib.now(), **entry}, ensure_ascii=False) + "\n")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--run")
    sub = parser.add_subparsers(dest="command", required=True)
    copy = sub.add_parser("copy")
    copy.add_argument("--block", required=True, choices=["S1", "S2", "S3", "S4"])
    touch = sub.add_parser("touch")
    touch.add_argument("path")
    group = touch.add_mutually_exclusive_group(required=True)
    group.add_argument("--mtime")
    group.add_argument("--days-ago", type=int)
    rm = sub.add_parser("rm")
    rm.add_argument("path")
    rm.add_argument("--recursive", action="store_true")
    mv = sub.add_parser("mv")
    mv.add_argument("source")
    mv.add_argument("target")
    chmod = sub.add_parser("chmod")
    chmod.add_argument("mode")
    chmod.add_argument("path")
    edge = sub.add_parser("write-edge")
    edge.add_argument("kind", choices=EDGE_KINDS)
    edge.add_argument("name")
    return parser


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    args = build_parser().parse_args(argv)
    try:
        repo = lib.repo_root(Path(__file__).resolve().parent)
        run_dir = lib.resolve_run(args.run, repo)
        run = lib.load_run(run_dir)
        lib.require_not_halted(run)
        ctx = session_context(repo, run)
        if args.command == "write-edge":
            blocks = lib.load_reference("qa-blocks.json")["blocks"]
            require(blocks.get(run["active_session"].get("block"), {}).get("edge"), "edge files are allowed only in S2")
        if os.environ.get(INNER) == "1":
            result = perform(ctx, args)
            print(json.dumps({"ok": True, **result}))
            return 0
        forwarded = argv if args.run else ["--run", str(run_dir), *argv]
        completed = subprocess.run(sandbox_argv(ctx["media"], os.path.abspath(__file__), forwarded),
                                   capture_output=True, text=True, timeout=1800)
        entry = {"session": ctx["session"], "argv": argv, "code": completed.returncode,
                 "stdout": completed.stdout[-2000:], "stderr": completed.stderr[-2000:]}
        log(run_dir, entry)
        sys.stdout.write(completed.stdout)
        sys.stderr.write(completed.stderr)
        return completed.returncode
    except (lib.AuditError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
