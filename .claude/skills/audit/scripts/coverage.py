#!/usr/bin/env python3
"""QA coverage: which UI controls the testers never touched.

  coverage.py [--run R] [--json]

Inventory from src/ (production .tsx/.ts, no tests): literal and translated aria-labels
(resolved through src/i18n/locales/en.json), data-testid, id, role attributes, and keyboard
keys compared in handlers (`key === "..."`). Touched selectors come from actions.jsonl of every
session recorded in run.json. Writes <run>/coverage.json and coverage.md. Controls reachable
only beyond the 16,000,000 px windowed threshold are flagged `scale_only`.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_lib as lib  # noqa: E402

ARIA_LITERAL = re.compile(r'aria-label="([^"]+)"')
ARIA_KEY = re.compile(r'aria-label=\{\s*t\(\s*"([a-zA-Z0-9_.-]+)"')
TESTID = re.compile(r'data-testid=(?:"([^"]+)"|\{`([^`]+)`\})')
ELEMENT_ID = re.compile(r'\bid="([a-zA-Z][a-zA-Z0-9_-]*)"')
ROLE = re.compile(r'\brole="([a-z]+)"')
KEY = re.compile(r'\bkey\s*===\s*"([^"]+)"')
SCALE_ONLY = {"gallery-scrollbar", "Gallery position"}


def flatten(tree: dict, prefix: str = "") -> dict[str, str]:
    flat: dict[str, str] = {}
    for key, value in tree.items():
        path = f"{prefix}.{key}" if prefix else key
        if isinstance(value, dict):
            flat.update(flatten(value, path))
        elif isinstance(value, str):
            flat[path] = value
    return flat


def inventory(repo: Path) -> list[dict]:
    locale = flatten(json.loads((repo / "src" / "i18n" / "locales" / "en.json").read_text(encoding="utf-8")))
    controls: dict[tuple[str, str], dict] = {}

    def add(kind: str, value: str, file: str) -> None:
        entry = controls.setdefault((kind, value), {"kind": kind, "value": value, "files": []})
        if file not in entry["files"]:
            entry["files"].append(file)

    for path in sorted((repo / "src").rglob("*.ts*")):
        rel = path.relative_to(repo).as_posix()
        if "__tests__" in rel or ".test." in rel or rel.startswith("src/test/"):
            continue
        text = path.read_text(encoding="utf-8")
        for match in ARIA_LITERAL.finditer(text):
            add("aria-label", match.group(1), rel)
        for match in ARIA_KEY.finditer(text):
            label = locale.get(match.group(1))
            if label and "{{" not in label:
                add("aria-label", label, rel)
        for match in TESTID.finditer(text):
            value = match.group(1) or re.sub(r"\$\{[^}]+\}", "*", match.group(2))
            add("data-testid", value, rel)
        for match in ELEMENT_ID.finditer(text):
            add("id", match.group(1), rel)
        for match in ROLE.finditer(text):
            add("role", match.group(1), rel)
        for match in KEY.finditer(text):
            add("key", match.group(1), rel)
    result = sorted(controls.values(), key=lambda c: (c["kind"], c["value"]))
    for control in result:
        control["scale_only"] = control["value"] in SCALE_ONLY
    return result


def touched_text(repo: Path, run: dict) -> str:
    chunks = []
    for session in run.get("sessions", []):
        actions = repo / "artifacts" / "app-control" / str(session.get("id")) / "actions.jsonl"
        if session.get("id") and actions.exists():
            for line in actions.read_text(encoding="utf-8", errors="replace").splitlines():
                try:
                    entry = json.loads(line)
                except ValueError:
                    continue
                if entry.get("phase") == "after" and entry.get("command") not in ("doctor", "logs", "screenshot"):
                    chunks.append(f"{entry.get('command')} {json.dumps(entry.get('args', []), ensure_ascii=False)}")
    return "\n".join(chunks)


def is_touched(control: dict, text: str) -> bool:
    value = control["value"]
    if control["kind"] == "data-testid":
        pattern = re.escape(value).replace(r"\*", r"[^\"'\]]*")
        return re.search(rf"data-testid=\\?[\"']?{pattern}", text) is not None
    if control["kind"] == "id":
        return f"#{value}" in text or f'id=\\"{value}\\"' in text
    if control["kind"] == "aria-label":
        return value in text
    if control["kind"] == "role":
        return f"role=\\\"{value}\\\"" in text or f"role={value}" in text
    return re.search(rf"\b{re.escape(value)}\b", text) is not None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--run")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)
    try:
        repo = lib.repo_root()
        run_dir = lib.resolve_run(args.run, repo)
        run = lib.load_run(run_dir)
    except lib.AuditError as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 2
    controls = inventory(repo)
    text = touched_text(repo, run)
    for control in controls:
        control["touched"] = is_touched(control, text)
    untouched = [c for c in controls if not c["touched"]]
    summary = {"generated": lib.now(), "controls": len(controls), "touched": len(controls) - len(untouched),
               "untouched": untouched}
    lib.write_json(run_dir / "coverage.json", summary)
    lines = ["# QA coverage", "", f"{summary['touched']}/{summary['controls']} controls touched.", "",
             "| Kind | Value | Scale only | Files |", "| --- | --- | --- | --- |"]
    lines += [f"| {c['kind']} | `{c['value']}` | {'yes' if c['scale_only'] else ''} | {', '.join(c['files'][:3])} |"
              for c in untouched]
    lib.atomic_write(run_dir / "coverage.md", "\n".join(lines) + "\n")
    print(json.dumps(summary if args.json else {k: summary[k] for k in ("controls", "touched")}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
