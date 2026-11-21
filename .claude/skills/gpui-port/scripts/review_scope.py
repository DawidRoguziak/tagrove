#!/usr/bin/env python3
"""The review scope of a gpui-port epic, reduced to what the epic workflow relays.

    review_scope.py --base REV

Runs .claude/skills/review-changes/scripts/collect_scope.py and prints one compact JSON object with
exactly the fields of the workflow's SCOPE schema. The clerk agent of the Review step returns this
object unchanged; it does not pick fields out of the larger collect_scope.py output itself, because a
summary that drops the file list makes the workflow choose the wrong review lenses.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

COLLECT = Path(__file__).resolve().parents[2] / "review-changes" / "scripts" / "collect_scope.py"


def reduce(scope: dict) -> dict:
    """The fields the workflow needs from one collect_scope.py result."""
    files = scope.get("files") or []
    totals = scope.get("totals") or {}
    return {
        "ok": True,
        "empty": bool(scope.get("empty")),
        "base": scope.get("base") or "",
        "target": scope.get("target") or "",
        "diff_command": scope.get("diff_command") or "",
        "untracked": [entry["path"] for entry in files if entry.get("status") == "A?"],
        "files": [{"path": entry["path"], "category": entry.get("category") or "",
                   "hot_path": bool(entry.get("hot_path"))} for entry in files],
        "file_count": len(files),
        "changed_lines": int(totals.get("changed_lines") or 0),
        "app_source_lines": int(totals.get("app_source_lines") or 0),
        "suggested_lenses": list(scope.get("suggested_lenses") or []),
        "rules": [entry["rule"] for entry in scope.get("rules") or []],
        "docs": list(scope.get("docs") or []),
    }


def failure(error: str) -> dict:
    return {"ok": False, "empty": False, "error": error.strip() or "collect_scope.py failed without a message"}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--base", required=True, help="the review base recorded by port_state.py args")
    options = parser.parse_args(argv)
    done = subprocess.run([sys.executable, str(COLLECT), "--base", options.base],
                          capture_output=True, text=True, check=False)
    if done.returncode != 0:
        print(json.dumps(failure(done.stderr or done.stdout)))
        return 1
    try:
        result = reduce(json.loads(done.stdout))
    except (ValueError, KeyError, TypeError) as error:
        print(json.dumps(failure(f"collect_scope.py printed no usable JSON: {error}")))
        return 1
    print(json.dumps(result, ensure_ascii=False, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
