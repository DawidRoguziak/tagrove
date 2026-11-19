#!/usr/bin/env python3
"""State of the GPUI port epics (docs/plans/gpui-port/epics.json).

  port_state.py status [--json]              Epics, statuses and the next runnable epic.
  port_state.py validate                     Check the manifest against the tree.
  port_state.py render                       Rewrite the epic table in the plan README.
  port_state.py args EPIC|next [--from STEP] [--models role=model[:effort],...]
        [--max-review-rounds N] [--max-fix-rounds N] [--max-gate-rounds N]
        [--allow-dirty] [--force] [--dry-run]
                                             Print the args object for Workflow gpui-port-epic.
  port_state.py set-status EPIC STATUS [--note TEXT] [--role ROLE]
                                             Update manifest, README table, epic file and NOTES.md.

The manifest is the source of truth. Run state (review base, job logs) lives in the ignored
docs/review/gpui-port/<EPIC>/.
"""
from __future__ import annotations

import argparse
import json
import re
import shlex
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
PLAN = "docs/plans/gpui-port"
RUNS = "docs/review/gpui-port"
MODELS = ".claude/skills/gpui-port/references/models.json"
TABLE_START, TABLE_END = "<!-- epics:start -->", "<!-- epics:end -->"
PROTECTED_BRANCHES = ("main", "master")
SHELL_TOKENS = ("&&", "||", "|", ";", ">", ">>", "<", "&")
CARGO_SUBCOMMANDS = ("test", "check", "clippy", "fmt", "tree", "build")
# Long, display-bound or interactive scripts never run as a check; the gate job owns test:all.
FORBIDDEN_BUN_SCRIPTS = ("test:all", "test:e2e:tauri", "tauri:dev", "gpui:dev", "app:control", "dev")


class PortError(Exception):
    pass


def repo_root() -> Path:
    return HERE.parents[3]


def today() -> str:
    return datetime.now(timezone.utc).date().isoformat()


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def load_manifest(root: Path) -> dict:
    return read_json(root / PLAN / "epics.json")


def find_epic(manifest: dict, epic_id: str) -> dict:
    for epic in manifest["epics"]:
        if epic["id"] == epic_id:
            return epic
    raise PortError(f"unknown epic {epic_id}")


def command_problem(command: str) -> str | None:
    """Return why a check command is not allowed, or None when it is."""
    try:
        tokens = shlex.split(command)
    except ValueError as error:
        return f"unparsable: {error}"
    if not tokens:
        return "empty command"
    if any(token in SHELL_TOKENS or "$(" in token or chr(96) in token for token in tokens):
        return "shell syntax is not allowed"
    head = tokens[0]
    if head == "cargo":
        if len(tokens) < 2 or tokens[1] not in CARGO_SUBCOMMANDS:
            return f"cargo subcommand must be one of {', '.join(CARGO_SUBCOMMANDS)}"
        if tokens[1] != "fmt" and "--locked" not in tokens:
            return "cargo commands need --locked"
        return None
    if head == "bun":
        if len(tokens) < 3 or tokens[1] != "run":
            return "only 'bun run <script>' is allowed"
        if tokens[2] in FORBIDDEN_BUN_SCRIPTS:
            return f"'bun run {tokens[2]}' is not a check"
        return None
    if head == "node":
        return None if len(tokens) >= 3 and tokens[1] == "--test" else "only 'node --test <path>' is allowed"
    if head == "python3":
        return None if tokens[1:3] == ["-m", "unittest"] else "only 'python3 -m unittest ...' is allowed"
    return f"{head} is not an allowed program"


def resolve_checks(manifest: dict, epic: dict) -> list[str]:
    commands: list[str] = []
    for entry in epic["checks"]:
        for command in manifest["checkSets"].get(entry, [entry]):
            if command not in commands:
                commands.append(command)
    return commands


def validate(root: Path, manifest: dict) -> list[str]:
    errors: list[str] = []
    ids = [epic["id"] for epic in manifest["epics"]]
    if len(ids) != len(set(ids)):
        errors.append("duplicate epic ids")
    known = set(ids)
    for name, commands in manifest["checkSets"].items():
        for command in commands:
            problem = command_problem(command)
            if problem:
                errors.append(f"checkSets.{name}: '{command}': {problem}")
    for epic in manifest["epics"]:
        tag = epic["id"]
        if not re.fullmatch(r"E\d\d", tag):
            errors.append(f"{tag}: id must look like E07")
        if epic["status"] not in manifest["statuses"]:
            errors.append(f"{tag}: unknown status {epic['status']}")
        if epic["tier"] not in ("hard", "standard"):
            errors.append(f"{tag}: tier must be hard or standard")
        for dep in epic["deps"]:
            if dep not in known:
                errors.append(f"{tag}: unknown dependency {dep}")
        if not (root / PLAN / epic["file"]).is_file():
            errors.append(f"{tag}: missing epic file {epic['file']}")
        for key in ("sources", "tests_sources", "docs"):
            for path in epic[key]:
                if not (root / path).exists():
                    errors.append(f"{tag}: {key} path does not exist: {path}")
        for scenario in epic["parity"]:
            if scenario not in manifest["scenarios"]:
                errors.append(f"{tag}: unknown scenario {scenario}")
        for command in resolve_checks(manifest, epic):
            problem = command_problem(command)
            if problem:
                errors.append(f"{tag}: check '{command}': {problem}")
    by_id = {epic["id"]: epic for epic in manifest["epics"]}
    state: dict[str, int] = {}

    def visit(node: str) -> bool:
        if state.get(node) == 1:
            return True
        if state.get(node) == 2:
            return False
        state[node] = 1
        cyclic = any(visit(dep) for dep in by_id[node]["deps"] if dep in by_id)
        state[node] = 2
        return cyclic

    if any(visit(epic_id) for epic_id in ids if epic_id in by_id):
        errors.append("dependency cycle")
    return errors


def table(manifest: dict) -> str:
    rows = ["| Epik | Nazwa | Rozmiar | Poziom | Zależy od | Status |", "| --- | --- | --- | --- | --- | --- |"]
    for epic in manifest["epics"]:
        deps = ", ".join(epic["deps"]) or "—"
        status = epic["status"] + (f" {epic['updated']}" if epic.get("updated") else "")
        tier = "trudny" if epic["tier"] == "hard" else "standardowy"
        rows.append(f"| {epic['id']} | [{epic['title']}]({epic['file']}) | {epic['size']} | {tier} | {deps} | {status} |")
    return "\n".join(rows)


def rendered_readme(text: str, manifest: dict) -> str:
    pattern = re.compile(re.escape(TABLE_START) + r".*?" + re.escape(TABLE_END), re.S)
    if not pattern.search(text):
        raise PortError("README has no epic table markers")
    return pattern.sub(lambda _: f"{TABLE_START}\n{table(manifest)}\n{TABLE_END}", text)


def render(root: Path, manifest: dict) -> None:
    readme = root / PLAN / "README.md"
    readme.write_text(rendered_readme(readme.read_text(encoding="utf-8"), manifest), encoding="utf-8")


def waiting_on(manifest: dict, epic: dict) -> list[str]:
    """Dependencies of the epic that are not done yet."""
    done = {item["id"] for item in manifest["epics"] if item["status"] == "done"}
    return [dep for dep in epic["deps"] if dep not in done]


def next_epic(manifest: dict) -> dict | None:
    for epic in manifest["epics"]:
        if epic["status"] in ("todo", "in progress", "blocked") and not waiting_on(manifest, epic):
            return epic
    return None


def set_status(root: Path, epic_id: str, status: str, note: str = "", role: str = "orkiestrator") -> dict:
    manifest = load_manifest(root)
    if status not in manifest["statuses"]:
        raise PortError(f"status must be one of {', '.join(manifest['statuses'])}")
    epic = find_epic(manifest, epic_id)
    epic["status"], epic["updated"] = status, today()
    write_json(root / PLAN / "epics.json", manifest)
    render(root, manifest)
    epic_file = root / PLAN / epic["file"]
    text = epic_file.read_text(encoding="utf-8")
    pattern = re.compile(r"(## Status\n\n).*?(\n\n## Notatki agentów)", re.S)
    if not pattern.search(text):
        raise PortError(f"{epic['file']} has no Status section")
    epic_file.write_text(pattern.sub(lambda m: f"{m.group(1)}{status} {today()}{m.group(2)}", text), encoding="utf-8")
    sentence = " ".join(note.split()).replace("|", "/") or "—"
    with open(root / PLAN / "NOTES.md", "a", encoding="utf-8") as notes:
        notes.write(f"{today()} | {epic_id} | {role} | {status} | {sentence} | [{epic_id}]({epic['file']})\n")
    return {"ok": True, "epic": epic_id, "status": status}


def git(root: Path, *args: str) -> str:
    result = subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True)
    if result.returncode != 0:
        raise PortError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def load_models(root: Path, overrides: str | None) -> dict:
    spec = read_json(root / MODELS)
    roles = {name: {"model": role["model"], "effort": role["effort"]} for name, role in spec["roles"].items()}
    for item in filter(None, (overrides or "").split(",")):
        if "=" not in item:
            raise PortError(f"bad --models entry '{item}'; expected role=model[:effort]")
        role, value = item.split("=", 1)
        model, _, effort = value.partition(":")
        if role not in roles:
            raise PortError(f"unknown role {role}; roles: {', '.join(roles)}")
        if model not in spec["models"]:
            raise PortError(f"unknown model {model}; models: {', '.join(spec['models'])}")
        if effort and effort not in spec["efforts"]:
            raise PortError(f"unknown effort {effort}; efforts: {', '.join(spec['efforts'])}")
        roles[role] = {"model": model, "effort": effort or roles[role]["effort"]}
    return roles


def run_dir(root: Path, epic_id: str) -> Path:
    path = root / RUNS / epic_id
    probe = subprocess.run(["git", "-C", str(root), "check-ignore", "-q", str(path / "x")])
    if probe.returncode != 0:
        raise PortError(f"{RUNS}/ must be git-ignored; refusing to write run state into tracked files")
    return path


def build_args(root: Path, options: argparse.Namespace) -> dict:
    manifest = load_manifest(root)
    errors = validate(root, manifest)
    if errors:
        raise PortError("manifest is invalid: " + "; ".join(errors[:5]))
    if options.epic == "next":
        epic = next_epic(manifest)
        if epic is None:
            raise PortError("no runnable epic: everything is done, deferred or waiting on a dependency")
    else:
        epic = find_epic(manifest, options.epic)
    step = options.start_from
    if step not in manifest["steps"]:
        raise PortError(f"--from must be one of {', '.join(manifest['steps'])}")
    dry = options.dry_run
    if not dry:
        if epic["status"] == "deferred" and not options.force:
            raise PortError(f"{epic['id']} is deferred; it needs a new decision and its own plan (--force to override)")
        if epic["status"] == "done" and not options.force:
            raise PortError(f"{epic['id']} is already done (--force to run it again)")
        waiting = waiting_on(manifest, epic)
        if waiting and not options.force:
            raise PortError(f"{epic['id']} waits on {', '.join(waiting)} (--force to override)")
        branch = git(root, "rev-parse", "--abbrev-ref", "HEAD")
        if branch in PROTECTED_BRANCHES:
            raise PortError(f"refusing to run on {branch}; switch to a feature branch")
    directory = run_dir(root, "_dryrun" if dry else epic["id"])
    state_path = directory / "run.json"
    state = read_json(state_path) if state_path.exists() else {}
    head = git(root, "rev-parse", "HEAD")
    if dry:
        base = head
    elif step == "implement":
        if git(root, "status", "--porcelain") and not options.allow_dirty:
            raise PortError("working tree is not clean; commit the previous epic first "
                            "(--allow-dirty continues an interrupted implement step)")
        base = state.get("base", head) if options.allow_dirty else head
    else:
        base = state.get("base")
        if not base:
            raise PortError(f"no earlier run of {epic['id']}; start from implement")
    state = {"epic": epic["id"], "base": base, "updated": today(),
             "history": [*state.get("history", []), {"date": today(), "from": step, "head": head}]}
    write_json(state_path, state)
    package = read_json(root / "package.json")
    if not dry and epic["status"] != "in progress":
        set_status(root, epic["id"], "in progress", f"workflow start od kroku {step}")
    keys = ("id", "slug", "title", "file", "size", "tier", "deps", "sources", "tests_sources", "docs",
            "allowed_deps", "parity")
    return {
        "epic": {**{key: epic[key] for key in keys}, "checks": resolve_checks(manifest, epic)},
        "plan": PLAN,
        "run": directory.relative_to(root).as_posix(),
        "base": base,
        "date": today(),
        "from": step,
        "maxFixRounds": options.max_fix_rounds,
        "maxReviewRounds": options.max_review_rounds,
        "maxGateRounds": options.max_gate_rounds,
        "maxParityRounds": 2,
        "parityAvailable": bool(epic["parity"]) and "compare:ui" in package.get("scripts", {}),
        "models": load_models(root, options.models),
        "dryRun": dry,
    }


def status(root: Path) -> dict:
    manifest = load_manifest(root)
    upcoming = next_epic(manifest)
    return {
        "epics": [{"id": epic["id"], "title": epic["title"], "status": epic["status"], "updated": epic.get("updated"),
                   "waits_on": waiting_on(manifest, epic)} for epic in manifest["epics"]],
        "next": upcoming["id"] if upcoming else None,
        "errors": validate(root, manifest),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    status_parser = sub.add_parser("status")
    status_parser.add_argument("--json", action="store_true")
    sub.add_parser("validate")
    sub.add_parser("render")
    args_parser = sub.add_parser("args")
    args_parser.add_argument("epic")
    args_parser.add_argument("--from", dest="start_from", default="implement")
    args_parser.add_argument("--models")
    args_parser.add_argument("--max-review-rounds", type=int, default=6)
    args_parser.add_argument("--max-fix-rounds", type=int, default=5)
    args_parser.add_argument("--max-gate-rounds", type=int, default=3)
    args_parser.add_argument("--allow-dirty", action="store_true")
    args_parser.add_argument("--force", action="store_true")
    args_parser.add_argument("--dry-run", action="store_true")
    set_parser = sub.add_parser("set-status")
    set_parser.add_argument("epic")
    set_parser.add_argument("status")
    set_parser.add_argument("--note", default="")
    set_parser.add_argument("--role", default="orkiestrator")
    options = parser.parse_args(argv)
    root = repo_root()
    try:
        if options.command == "status":
            result = status(root)
            if options.json:
                print(json.dumps(result, indent=2, ensure_ascii=False))
            else:
                for epic in result["epics"]:
                    waits = ""
                    if epic["waits_on"] and epic["status"] not in ("done", "deferred"):
                        waits = f"  (czeka na {', '.join(epic['waits_on'])})"
                    print(f"{epic['id']}  {epic['status']:<12} {epic['title']}{waits}")
                print(f"next: {result['next'] or '-'}")
                for error in result["errors"]:
                    print(f"manifest error: {error}", file=sys.stderr)
            return 1 if result["errors"] else 0
        if options.command == "validate":
            errors = validate(root, load_manifest(root))
            print(json.dumps({"ok": not errors, "errors": errors}, indent=2, ensure_ascii=False))
            return 1 if errors else 0
        if options.command == "render":
            render(root, load_manifest(root))
            result = {"ok": True}
        elif options.command == "args":
            result = build_args(root, options)
        else:
            result = set_status(root, options.epic, options.status, options.note, options.role)
    except (PortError, OSError, json.JSONDecodeError) as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
