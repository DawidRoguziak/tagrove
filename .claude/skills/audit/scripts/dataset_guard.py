#!/usr/bin/env python3
"""Prove that the audit dataset outside `for-claude-tests/` never changes.

  snapshot --dataset D --out F          Write a manifest (relpath, type, size, mtime_ns,
                                        ctime_ns, inode, link target) without following links.
  check --dataset D --manifest F [--run R] [--label L]
                                        Re-snapshot and compare. Also report leftover
                                        `.mediatagger-*.pending` and `.*.tmp` files anywhere
                                        outside `for-claude-tests/`. Exit 0 clean, 3 dirty.
                                        With --run, a dirty result sets run.json.halted and the
                                        report is stored under <run>/guard/.
Read-only on the dataset: only lstat/readdir/readlink are used.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import stat
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_lib as lib  # noqa: E402

WRITABLE = "for-claude-tests"
SUSPICIOUS = re.compile(r"^(\.mediatagger-.*\.pending|\..*\.tmp)$")


def snapshot(dataset: Path) -> dict:
    dataset = Path(os.path.realpath(dataset))
    if not dataset.is_dir():
        raise lib.AuditError("dataset is not a directory")
    entries: dict[str, dict] = {}
    suspicious: list[str] = []

    def visit(directory: str, relative: str) -> None:
        try:
            names = sorted(os.listdir(directory))
        except OSError as error:
            entries[relative or "."]["error"] = error.strerror
            return
        for name in names:
            rel = f"{relative}/{name}" if relative else name
            if rel == WRITABLE:
                continue
            path = os.path.join(directory, name)
            info = os.lstat(path)
            entry = {
                "type": "dir" if stat.S_ISDIR(info.st_mode) else "link" if stat.S_ISLNK(info.st_mode)
                else "file" if stat.S_ISREG(info.st_mode) else "other",
                "size": info.st_size,
                "mtime_ns": info.st_mtime_ns,
                "ctime_ns": info.st_ctime_ns,
                "inode": info.st_ino,
                "mode": stat.S_IMODE(info.st_mode),
            }
            if entry["type"] == "link":
                entry["target"] = os.readlink(path)
            entries[rel] = entry
            if SUSPICIOUS.match(name):
                suspicious.append(rel)
            if entry["type"] == "dir":
                visit(path, rel)

    root = os.lstat(dataset)
    entries["."] = {"type": "dir", "mtime_ns": root.st_mtime_ns, "ctime_ns": root.st_ctime_ns,
                    "inode": root.st_ino, "mode": stat.S_IMODE(root.st_mode), "size": root.st_size}
    visit(str(dataset), "")
    # The root's own mtime/ctime change when for-claude-tests/ is touched; compare only identity.
    entries["."].pop("mtime_ns")
    entries["."].pop("ctime_ns")
    entries["."].pop("size")
    files = sum(1 for e in entries.values() if e["type"] == "file")
    return {"schema": 1, "created": lib.now(), "entries": entries, "suspicious": suspicious,
            "counts": {"entries": len(entries), "files": files,
                       "bytes": sum(e.get("size", 0) for e in entries.values() if e["type"] == "file")}}


def compare(before: dict, after: dict) -> dict:
    old, new = before["entries"], after["entries"]
    added = sorted(set(new) - set(old))
    removed = sorted(set(old) - set(new))
    changed = []
    for rel in sorted(set(old) & set(new)):
        diff = sorted(key for key in set(old[rel]) | set(new[rel]) if old[rel].get(key) != new[rel].get(key))
        if diff:
            changed.append({"path": rel, "fields": diff})
    clean = not (added or removed or changed or after["suspicious"])
    return {"clean": clean, "added": added[:200], "removed": removed[:200], "changed": changed[:200],
            "suspicious": after["suspicious"][:200],
            "totals": {"added": len(added), "removed": len(removed), "changed": len(changed),
                       "suspicious": len(after["suspicious"])}}


def check(dataset: Path, manifest: Path, run_dir: Path | None, label: str) -> dict:
    result = compare(lib.read_json(manifest), snapshot(dataset))
    result.update(label=label, time=lib.now())
    if run_dir is not None:
        guard_dir = run_dir / "guard"
        guard_dir.mkdir(exist_ok=True)
        index = len(list(guard_dir.glob("*.json"))) + 1
        safe_label = re.sub(r"[^A-Za-z0-9_-]+", "-", label)[:40] or "check"
        lib.write_json(guard_dir / f"{index:03d}-{safe_label}.json", result)
        if not result["clean"]:
            lib.halt(run_dir, f"dataset guard '{label}' found changes outside for-claude-tests "
                     f"({json.dumps(result['totals'])})", "dataset_guard")
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    snap = sub.add_parser("snapshot")
    snap.add_argument("--dataset", required=True)
    snap.add_argument("--out", required=True)
    chk = sub.add_parser("check")
    chk.add_argument("--dataset")
    chk.add_argument("--manifest")
    chk.add_argument("--run")
    chk.add_argument("--label", default="check")
    args = parser.parse_args(argv)
    try:
        if args.command == "snapshot":
            lib.write_json(Path(args.out), snapshot(Path(args.dataset)))
            print(json.dumps({"ok": True, "out": args.out}))
            return 0
        run_dir = lib.resolve_run(args.run) if args.run or not (args.dataset and args.manifest) else None
        dataset = Path(args.dataset) if args.dataset else Path(lib.load_run(run_dir)["dataset"])
        manifest = Path(args.manifest) if args.manifest else run_dir / "context" / "dataset-manifest.json"
        result = check(dataset, manifest, run_dir, args.label)
    except lib.AuditError as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 2
    print(json.dumps({k: result[k] for k in ("clean", "totals", "label")}))
    return 0 if result["clean"] else 3


if __name__ == "__main__":
    sys.exit(main())
