#!/usr/bin/env python3
"""Clear run.json.halted after a human inspected an /audit run.

  clear_halt.py [--run R] --confirm-inspected

Deliberately separate from audit_state.py and absent from every permission allowlist, so the
harness always asks the user before it runs. Agents never run it (qa_hook.py refuses it).
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_lib as lib  # noqa: E402


def clear(run_dir: Path) -> dict:
    def mutate(run: dict) -> None:
        if run.get("halted"):
            run.setdefault("halt_history", []).append({**run["halted"], "cleared": lib.now()})
        run["halted"] = None

    lib.update_run(run_dir, mutate)
    return {"ok": True, "halted": None}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--run")
    parser.add_argument("--confirm-inspected", action="store_true", required=True)
    args = parser.parse_args(argv)
    try:
        result = clear(lib.resolve_run(args.run))
    except lib.AuditError as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 2
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
