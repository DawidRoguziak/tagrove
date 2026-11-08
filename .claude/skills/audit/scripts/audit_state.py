#!/usr/bin/env python3
"""State of one /audit run: pending work, validated write-once results, IDs, status, models.

Subcommands (all accept --run <dir|name>; default docs/review/LATEST):
  pending --ids <stage> [--block S1] [--models role=model[:effort],...] [--include-optional]
      Print {stage, run, items, models} for the coordinator to pass as Workflow args.
  put <kind> [--file F]           Validate JSON (stdin by default) and store it write-once.
      kinds: raw, dedup, tools, refutation, judgment, plan, qa-suite, triage, qa-dedup, report
  assign-ids --kind static|qa     Turn raw results into records S-*/Q-* (stable IDs, merged_from).
  dup-index                       Group records by file and nearby lines for judges.
  hook-check --prepare|--evaluate Automatic proof that the QA hook blocks a qa-tester agent:
                                  prepare the probe folder, then evaluate what the probe agent left.
  status                          Regenerate STATUS.md, static-findings.md, qa-findings.md.
  models [--role R] [--record ID] [--models overrides]
                                  Resolve roles to {model, effort}.
Agents may only call `put` (and read-only subcommands). Scripts set `halted`, never agents.
"""
from __future__ import annotations

import argparse
import glob
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_lib as lib  # noqa: E402

PRIORITIES = ["P0", "P1", "P2", "P3"]
CATEGORIES = ["bug", "perf", "stability", "convention", "data-safety"]
STATIC_ORIGINS = ["pre-existing", "recon", "tool"]
STATIC_VERDICTS = ["confirmed", "rejected", "duplicate", "known", "unproven"]
QA_VERDICTS = ["confirmed", "rejected", "not-reproducible", "duplicate", "known"]
REFUTER_VERDICTS = ["confirmed", "rejected", "uncertain"]
QA_GROUNDS = ["none", "intended", "known", "artifact", "not-reproducible"]
STAGES = ["static", "static-verify", "qa-prep", "qa-run", "qa-verify", "report"]
BLOCKS = ["S1", "S2", "S3", "S4"]
SUITE_ID = re.compile(r"^QA-(\d{2}|T\d{1,3})$")
RECORD_ID = re.compile(r"^[SQ]-[0-9a-f]{8}$")
BATCH_ID = re.compile(r"^B\d{1,2}[a-z]?$")
SAFE_NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,80}$")


# ---------------------------------------------------------------- validation helpers


class Invalid(lib.AuditError):
    pass


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise Invalid(message)


def _str(value: Any, name: str, *, empty: bool = False, limit: int = 20000) -> str:
    _require(isinstance(value, str), f"{name} must be a string")
    _require(empty or value.strip() != "", f"{name} must not be empty")
    _require(len(value) <= limit, f"{name} is longer than {limit} characters")
    return value


def _opt_str(value: Any, name: str) -> str | None:
    return None if value is None else _str(value, name)


def _enum(value: Any, allowed: list[str], name: str) -> str:
    _require(value in allowed, f"{name} must be one of {', '.join(allowed)}")
    return value


def _str_list(value: Any, name: str, *, min_items: int = 0) -> list[str]:
    _require(isinstance(value, list), f"{name} must be a list")
    _require(len(value) >= min_items, f"{name} needs at least {min_items} item(s)")
    return [_str(item, f"{name}[]") for item in value]


def _repo_path(value: Any, name: str) -> str:
    text = _str(value, name, limit=400)
    _require(not text.startswith("/") and ".." not in Path(text).parts, f"{name} must be repo-relative")
    _require("/home/" not in text, f"{name} must not contain a home path")
    return text


def _object(value: Any, name: str) -> dict:
    _require(isinstance(value, dict), f"{name} must be an object")
    return value


def validate_static_finding(item: Any, name: str = "finding") -> dict:
    f = _object(item, name)
    _str(f.get("title"), f"{name}.title", limit=300)
    _repo_path(f.get("file"), f"{name}.file")
    _require(isinstance(f.get("line"), int) and f["line"] >= 1, f"{name}.line must be a positive integer")
    if f.get("end_line") is not None:
        _require(isinstance(f["end_line"], int) and f["end_line"] >= f["line"], f"{name}.end_line invalid")
    _enum(f.get("category"), CATEGORIES, f"{name}.category")
    _enum(f.get("priority"), PRIORITIES, f"{name}.priority")
    _enum(f.get("origin", "pre-existing"), STATIC_ORIGINS, f"{name}.origin")
    for key in ("trigger", "mechanism", "impact"):
        _str(f.get(key), f"{name}.{key}")
    _str_list(f.get("evidence"), f"{name}.evidence", min_items=1)
    return f


def validate_qa_bug(item: Any, name: str = "bug") -> dict:
    b = _object(item, name)
    _str(b.get("title"), f"{name}.title", limit=300)
    _str(b.get("area"), f"{name}.area", limit=120)
    _str_list(b.get("steps"), f"{name}.steps", min_items=1)
    _str(b.get("expected"), f"{name}.expected")
    _opt_str(b.get("expected_ref"), f"{name}.expected_ref")
    _str(b.get("actual"), f"{name}.actual")
    _str_list(b.get("evidence"), f"{name}.evidence", min_items=1)
    _require(isinstance(b.get("frequency"), str) and re.fullmatch(r"\d+/\d+", b["frequency"]) is not None,
             f"{name}.frequency must look like 2/2")
    _enum(b.get("priority"), PRIORITIES, f"{name}.priority")
    _enum(b.get("category"), CATEGORIES, f"{name}.category")
    _opt_str(b.get("suspect"), f"{name}.suspect")
    return b


def validate_raw(data: Any) -> dict:
    d = _object(data, "raw")
    _require(BATCH_ID.match(str(d.get("batch", ""))) is not None, "raw.batch must be a batch ID")
    _require(SAFE_NAME.match(str(d.get("lens", ""))) is not None, "raw.lens must be a lens name")
    findings = d.get("findings")
    _require(isinstance(findings, list), "raw.findings must be a list (empty when nothing survived)")
    for index, finding in enumerate(findings):
        validate_static_finding(finding, f"findings[{index}]")
        _require(finding.get("origin", "pre-existing") == "pre-existing", "finder findings use origin pre-existing")
    for index, lead in enumerate(d.get("leads", [])):
        _str(_object(lead, f"leads[{index}]").get("title"), f"leads[{index}].title")
    _str_list(d.get("files_reviewed", []), "raw.files_reviewed")
    return d


def validate_dedup(data: Any) -> dict:
    d = _object(data, "dedup")
    _require(BATCH_ID.match(str(d.get("batch", ""))) is not None, "dedup.batch must be a batch ID")
    _validate_groups(d.get("groups"), r"^[A-Za-z0-9]+:[a-z]+#\d+$")
    return d


def _validate_groups(groups: Any, ref_pattern: str) -> None:
    _require(isinstance(groups, list), "groups must be a list")
    seen: set[str] = set()
    for index, group in enumerate(groups):
        g = _object(group, f"groups[{index}]")
        refs = [g.get("keep"), *(g.get("merged") or [])]
        _require(isinstance(g.get("merged"), list) and g["merged"], f"groups[{index}].merged must be non-empty")
        for ref in refs:
            _require(isinstance(ref, str) and re.match(ref_pattern, ref) is not None, f"bad reference {ref!r}")
            _require(ref not in seen, f"reference {ref} appears in two groups")
            seen.add(ref)
        _str(g.get("reason"), f"groups[{index}].reason")


def validate_tools(data: Any) -> dict:
    d = _object(data, "tools")
    _require(isinstance(d.get("results"), list) and d["results"], "tools.results must be a non-empty list")
    for index, result in enumerate(d["results"]):
        r = _object(result, f"results[{index}]")
        _require(SAFE_NAME.match(str(r.get("name", ""))) is not None, f"results[{index}].name invalid")
        _enum(r.get("status"), ["pass", "fail", "unavailable", "timeout"], f"results[{index}].status")
        for f_index, failure in enumerate(r.get("failures", [])):
            validate_static_finding({**failure, "origin": "tool"}, f"results[{index}].failures[{f_index}]")
    return d


def validate_refutation(data: Any, record: dict) -> dict:
    d = _object(data, "refutation")
    _enum(d.get("verdict"), REFUTER_VERDICTS, "refutation.verdict")
    _enum(d.get("priority"), PRIORITIES, "refutation.priority")
    _str(d.get("reasoning"), "refutation.reasoning")
    _str_list(d.get("evidence"), "refutation.evidence", min_items=1)
    _str_list(d.get("counterevidence", []), "refutation.counterevidence")
    if record["kind"] == "qa":
        _enum(d.get("grounds", "none"), QA_GROUNDS, "refutation.grounds")
        if d.get("reproduced") is not None:
            _require(re.fullmatch(r"\d+/\d+", str(d["reproduced"])) is not None, "refutation.reproduced like 1/2")
    if d["verdict"] == "uncertain":
        _str(d.get("missing_fact"), "refutation.missing_fact (required when uncertain)")
    if d.get("new_issue") is not None:
        _object(d["new_issue"], "refutation.new_issue")
    return d


def validate_judgment(data: Any, record: dict) -> dict:
    d = _object(data, "judgment")
    verdicts = STATIC_VERDICTS if record["kind"] == "static" else QA_VERDICTS
    _enum(d.get("verdict"), verdicts, "judgment.verdict")
    _enum(d.get("priority"), PRIORITIES, "judgment.priority")
    _enum(d.get("category"), CATEGORIES, "judgment.category")
    _str(d.get("reasoning"), "judgment.reasoning")
    _str_list(d.get("evidence"), "judgment.evidence", min_items=1)
    if d["verdict"] == "duplicate":
        _require(RECORD_ID.match(str(d.get("duplicate_of", ""))) is not None, "duplicate needs duplicate_of")
        _require(d["duplicate_of"] != record["id"], "a record cannot duplicate itself")
    if d["verdict"] == "known":
        _str(d.get("known_source"), "known needs known_source (a file from known-sources.md)")
    if d["verdict"] == "unproven":
        _str(d.get("missing_fact"), "unproven needs missing_fact")
    return d


def validate_plan(data: Any) -> dict:
    d = _object(data, "plan")
    _require(isinstance(d.get("suites"), list) and d["suites"], "plan.suites must be a non-empty list")
    seen: set[str] = set()
    for index, suite in enumerate(d["suites"]):
        s = _object(suite, f"suites[{index}]")
        _require(SUITE_ID.match(str(s.get("id", ""))) is not None, f"suites[{index}].id must be QA-NN or QA-TN")
        _require(s["id"] not in seen, f"duplicate suite {s['id']}")
        seen.add(s["id"])
        _enum(s.get("block"), BLOCKS, f"suites[{index}].block")
        _str(s.get("title"), f"suites[{index}].title", limit=200)
        _str(s.get("scenario_ref"), f"suites[{index}].scenario_ref", limit=200)
        _require(isinstance(s.get("destructive"), bool), f"suites[{index}].destructive must be boolean")
        _require(isinstance(s.get("perf_build", False), bool), f"suites[{index}].perf_build must be boolean")
        for target in s.get("targets", []):
            _require(RECORD_ID.match(str(target)) is not None, f"suites[{index}].targets has a bad record ID")
        _str_list(s.get("focus", []), f"suites[{index}].focus")
    return d


def validate_qa_suite(data: Any) -> dict:
    d = _object(data, "qa-suite")
    _enum(d.get("block"), BLOCKS, "qa-suite.block")
    _require(SUITE_ID.match(str(d.get("suite", ""))) is not None, "qa-suite.suite must be QA-NN or QA-TN")
    _enum(d.get("status"), ["completed", "partial", "blocked", "aborted"], "qa-suite.status")
    _require(re.fullmatch(r"[a-f0-9]{32}", str(d.get("session", ""))) is not None, "qa-suite.session must be the session ID")
    _str(d.get("summary"), "qa-suite.summary")
    _str_list(d.get("scenarios_run", []), "qa-suite.scenarios_run")
    _str_list(d.get("scenarios_skipped", []), "qa-suite.scenarios_skipped")
    _require(isinstance(d.get("bugs"), list), "qa-suite.bugs must be a list")
    for index, bug in enumerate(d["bugs"]):
        validate_qa_bug(bug, f"bugs[{index}]")
    return d


def validate_triage(data: Any) -> dict:
    d = _object(data, "triage")
    _enum(d.get("block"), BLOCKS, "triage.block")
    _require(isinstance(d.get("assignments"), list), "triage.assignments must be a list")
    for index, item in enumerate(d["assignments"]):
        a = _object(item, f"assignments[{index}]")
        _str(a.get("suite"), f"assignments[{index}].suite")
        _enum(a.get("source"), ["console", "process", "sampler", "actions"], f"assignments[{index}].source")
        _str(a.get("text"), f"assignments[{index}].text")
    for index, bug in enumerate(d.get("new_candidates", [])):
        validate_qa_bug(bug, f"new_candidates[{index}]")
        _require(SUITE_ID.match(str(bug.get("suite", ""))) is not None, f"new_candidates[{index}].suite missing")
    return d


def validate_qa_dedup(data: Any) -> dict:
    d = _object(data, "qa-dedup")
    _validate_groups(d.get("groups"), r"^S[1-4]/QA-(\d{2}|T\d{1,3})#[a-z]*\d+$")
    return d


# ---------------------------------------------------------------- models


def parse_overrides(text: str | None) -> dict[str, dict[str, str]]:
    config = lib.load_reference("models.json")
    result: dict[str, dict[str, str]] = {}
    if not text:
        return result
    for part in text.split(","):
        part = part.strip()
        if not part:
            continue
        match = re.fullmatch(r"([a-z_]+)=([a-z]+)(?::([a-z]+))?", part)
        _require(match is not None, f"override {part!r} must look like role=model[:effort]")
        role, model, effort = match.groups()
        _require(role in config["roles"], f"unknown role {role}")
        _require(model in config["models"], f"unknown model {model}")
        if effort is not None:
            _require(effort in config["efforts"], f"unknown effort {effort}")
        result[role] = {"model": model, **({"effort": effort} if effort else {})}
    return result


def resolve_models(overrides: str | None = None, stored: dict | None = None) -> dict[str, dict[str, str]]:
    config = lib.load_reference("models.json")
    resolved = {role: {"model": spec["model"], "effort": spec["effort"]} for role, spec in config["roles"].items()}
    for source in (stored or {}, parse_overrides(overrides)):
        for role, spec in source.items():
            resolved[role] = {**resolved[role], **spec}
    return resolved


def severity(priority: str | None) -> int:
    return PRIORITIES.index(priority) if priority in PRIORITIES else len(PRIORITIES)


def is_dispute(record: dict) -> bool:
    refutation = record.get("refutation") or {}
    finder = record["finding"]["priority"]
    verdict = refutation.get("verdict")
    if verdict == "uncertain":
        return True
    if verdict == "rejected" and severity(finder) <= 1:
        return True
    if refutation.get("priority") and abs(severity(finder) - severity(refutation["priority"])) >= 2:
        return True
    return False


def refuter_role(record: dict) -> str:
    if record["kind"] == "static":
        return "static_refuter_high" if severity(record["finding"]["priority"]) <= 1 else "static_refuter_low"
    finding = record["finding"]
    if finding.get("block") == "S2" or finding.get("category") == "data-safety":
        return "qa_refuter_safety"
    return "qa_refuter"


def judge_role(record: dict) -> str:
    refutation = record.get("refutation") or {}
    top = min(severity(record["finding"]["priority"]), severity(refutation.get("priority")))
    return "judge_high" if top <= 1 or is_dispute(record) else "judge_low"


# ---------------------------------------------------------------- records


def norm_title(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", " ", title.lower()).strip()


def static_id(finding: dict) -> str:
    key = f"{finding['file']}:{finding['line']}:{norm_title(finding['title'])}"
    return "S-" + hashlib.sha1(key.encode("utf-8")).hexdigest()[:8]


def qa_id(block: str, suite: str, title: str) -> str:
    key = f"{block}:{suite}:{norm_title(title)}"
    return "Q-" + hashlib.sha1(key.encode("utf-8")).hexdigest()[:8]


def record_path(run_dir: Path, record_id: str) -> Path:
    _require(RECORD_ID.match(record_id) is not None, f"bad record ID {record_id}")
    return run_dir / "records" / f"{record_id}.json"


def load_record(run_dir: Path, record_id: str) -> dict:
    path = record_path(run_dir, record_id)
    _require(path.is_file(), f"unknown record {record_id}")
    return lib.read_json(path)


def all_records(run_dir: Path, kind: str | None = None) -> list[dict]:
    records = [lib.read_json(path) for path in sorted((run_dir / "records").glob("*.json"))]
    return [record for record in records if kind is None or record["kind"] == kind]


def upsert_record(run_dir: Path, record_id: str, kind: str, finding: dict, sources: list[str], extra: dict) -> str:
    """Create a candidate, or add new sources to an existing record. Returns created|merged|unchanged."""
    path = record_path(run_dir, record_id)
    if path.exists():
        record = lib.read_json(path)
        new = [source for source in sources if source not in record["sources"]]
        if not new:
            return "unchanged"
        record["sources"].extend(new)
        record["merged_from"] = sorted(set(record.get("merged_from", [])) | set(new))
        lib.write_json(path, record)
        return "merged"
    record = {
        "schema": lib.SCHEMA_VERSION,
        "id": record_id,
        "kind": kind,
        "state": "candidate",
        "finding": finding,
        "sources": sources,
        "merged_from": sources[1:],
        "refutation": None,
        "judgment": None,
        "history": [{"state": "candidate", "time": lib.now()}],
        **extra,
    }
    lib.write_json(path, record, once=True)
    return "created"


# ---------------------------------------------------------------- batches and pending


def load_batches(run_dir: Path) -> dict:
    stored = run_dir / "context" / "batches.json"
    return lib.read_json(stored) if stored.is_file() else lib.load_reference("batches.json")


def batch_files(repo: Path, batch: dict) -> list[str]:
    files: set[str] = set()
    for pattern in batch["globs"]:
        for match in glob.glob(str(repo / pattern), recursive=True):
            path = Path(match)
            if path.is_file():
                files.add(path.relative_to(repo).as_posix())
    excluded = set(batch.get("exclude", []))
    if not batch.get("tests"):
        files = {f for f in files if "__tests__" not in f and ".test." not in f}
    return sorted(f for f in files if f not in excluded)


def lens_role(batch: dict, lens: str) -> str:
    if lens in ("conventions", "tooling"):
        return "finder_rules"
    return "finder_critical" if batch.get("critical") else "finder"


def finder_items(repo: Path, run_dir: Path, include_optional: bool) -> list[dict]:
    config = load_batches(run_dir)
    items = []
    for batch in config["batches"]:
        if batch.get("optional") and not include_optional:
            continue
        files = batch_files(repo, batch)
        for lens in batch["lenses"]:
            raw = run_dir / "raw" / f"{batch['id']}-{lens}.json"
            items.append({
                "id": f"{batch['id']}:{lens}",
                "type": "finder",
                "done": raw.is_file(),
                "batch": batch["id"],
                "title": batch["title"],
                "lens": lens,
                "agent": config["lens_agents"][lens],
                "role": lens_role(batch, lens),
                "critical": bool(batch.get("critical")),
                "files": files,
            })
    return items


def pending_static(repo: Path, run_dir: Path, include_optional: bool) -> list[dict]:
    finders = finder_items(repo, run_dir, include_optional)
    items: list[dict] = []
    if not (run_dir / "tools" / "summary.json").is_file():
        items.append({"id": "tools", "type": "tools", "role": "clerk"})
    by_batch: dict[str, list[dict]] = {}
    for item in finders:
        by_batch.setdefault(item["batch"], []).append(item)
    for batch, lenses in by_batch.items():
        todo = [item for item in lenses if not item["done"]]
        for item in todo:
            items.append({key: value for key, value in item.items() if key != "done"})
        if len(lenses) > 1 and not (run_dir / "raw" / f"{batch}.dedup.json").is_file():
            items.append({
                "id": f"{batch}:dedup",
                "type": "dedup",
                "batch": batch,
                "role": "clerk",
                "after": [item["id"] for item in todo],
                "raw": [f"raw/{batch}-{item['lens']}.json" for item in lenses],
            })
    return items


def pending_verify(run_dir: Path, kind: str) -> list[dict]:
    items = []
    for record in all_records(run_dir, kind):
        if record["state"] == "judged":
            continue
        item = {"id": record["id"], "state": record["state"], "priority": record["finding"]["priority"]}
        if record["state"] == "candidate":
            item.update(needs="refute", role=refuter_role(record))
        else:
            item.update(needs="judge", role=judge_role(record))
        if kind == "qa":
            item["block"] = record["finding"].get("block")
            item["category"] = record["finding"].get("category")
        items.append(item)
    return items


def qa_bug_refs(run_dir: Path) -> list[tuple[str, str, str, dict]]:
    """(ref, block, suite, bug) for every tester bug, triage candidate and failed sweep spec."""
    refs = []
    for report in sorted((run_dir / "qa" / "runs").glob("S*/QA-*.json")):
        data = lib.read_json(report)
        for index, bug in enumerate(data["bugs"]):
            refs.append((f"{data['block']}/{data['suite']}#{index}", data["block"], data["suite"], bug))
    for triage in sorted((run_dir / "qa" / "runs").glob("S*/triage.json")):
        data = lib.read_json(triage)
        for index, bug in enumerate(data.get("new_candidates", [])):
            refs.append((f"{data['block']}/{bug['suite']}#t{index}", data["block"], bug["suite"], bug))
    return refs


def sweep_failures(run_dir: Path) -> list[dict]:
    failures = []
    for path in sorted((run_dir / "qa" / "sweep").glob("*.json")):
        data = lib.read_json(path)
        if data.get("status") == "failed":
            failures.append(data)
    return failures


def pending_qa_prep(run_dir: Path) -> list[dict]:
    items = []
    if not (run_dir / "qa" / "sweep" / "summary.json").is_file():
        items.append({"id": "sweep", "type": "sweep", "role": "clerk"})
    if not (run_dir / "qa" / "plan.json").is_file():
        unproven = [r["id"] for r in all_records(run_dir, "static")
                    if r.get("judgment") and r["judgment"]["verdict"] in ("unproven", "confirmed")]
        items.append({"id": "plan", "type": "plan", "role": "qa_planner", "static_targets": unproven})
    return items


def pending_qa_run(run_dir: Path, block_filter: str | None) -> list[dict]:
    plan_path = run_dir / "qa" / "plan.json"
    _require(plan_path.is_file(), "qa/plan.json is missing; run qa-prep first")
    plan = lib.read_json(plan_path)
    blocks = lib.load_reference("qa-blocks.json")["blocks"]
    items = []
    for block in BLOCKS:
        if block_filter and block != block_filter:
            continue
        suites = [s for s in plan["suites"] if s["block"] == block
                  and not (run_dir / "qa" / "runs" / block / f"{s['id']}.json").is_file()]
        if not suites:
            continue
        sessions: list[dict] = []
        for perf in (False, True):
            group = [s for s in suites if bool(s.get("perf_build")) == perf]
            if group:
                sessions.append({"perf": perf, "suites": group})
        items.append({
            "id": block,
            "type": "block",
            "title": blocks[block]["title"],
            "tester_role": blocks[block]["tester_role"],
            "copy": blocks[block]["copy"],
            "edge": blocks[block]["edge"],
            "sessions": sessions,
            "triage_done": (run_dir / "qa" / "runs" / block / "triage.json").is_file(),
        })
    return items


def pending_qa_verify(run_dir: Path) -> list[dict]:
    has_bugs = bool(qa_bug_refs(run_dir))
    if has_bugs and not (run_dir / "qa" / "dedup.json").is_file():
        return [{"id": "qa-dedup", "type": "qa-dedup", "role": "qa_dedup",
                 "refs": [ref for ref, *_ in qa_bug_refs(run_dir)]}]
    return pending_verify(run_dir, "qa")


def cmd_pending(args, repo: Path, run_dir: Path) -> dict:
    run = lib.load_run(run_dir)
    lib.require_not_halted(run)
    if args.ids.startswith("qa-") and args.ids != "qa-prep":
        missing = hooks_missing(repo)
        _require(not missing, "QA safety hooks are not installed: " + "; ".join(missing))
        _require((run.get("hook_smoke") or {}).get("status") == "pass",
                 "the automatic QA hook smoke has not passed (audit_state.py hook-check)")
    if args.ids == "static":
        items = pending_static(repo, run_dir, args.include_optional)
    elif args.ids == "static-verify":
        items = pending_verify(run_dir, "static")
    elif args.ids == "qa-prep":
        items = pending_qa_prep(run_dir)
    elif args.ids == "qa-run":
        _require(not run.get("active_session"), "an active QA session is recorded; run qa_session.py reap first")
        items = pending_qa_run(run_dir, args.block)
    elif args.ids == "qa-verify":
        items = pending_qa_verify(run_dir)
    else:
        items = [] if (run_dir / "report.md").is_file() else [{"id": "report", "type": "report", "role": "report_editor"}]
    return {
        "stage": args.ids,
        "run": run_dir.relative_to(repo).as_posix(),
        "items": items,
        "models": resolve_models(args.models, run.get("model_overrides")),
    }


def hooks_missing(repo: Path) -> list[str]:
    """Both QA hook layers must be installed before any QA session (see safety.md)."""
    missing = []
    for agent in ("qa-tester", "qa-refuter"):
        path = repo / ".claude" / "agents" / f"{agent}.md"
        text = path.read_text(encoding="utf-8") if path.is_file() else ""
        head = text.split("\n---", 1)[0]
        if "hooks:" not in head or "qa_hook.py" not in head:
            missing.append(f"{agent}.md frontmatter has no qa_hook.py hook")
    settings = repo / ".claude" / "settings.local.json"
    try:
        hooks = json.loads(settings.read_text(encoding="utf-8")).get("hooks", {}).get("PreToolUse", [])
    except (OSError, ValueError):
        hooks = []
    if "qa_hook.py" not in json.dumps(hooks):
        missing.append(".claude/settings.local.json has no PreToolUse qa_hook.py hook")
    return missing


# ---------------------------------------------------------------- put


def sanitize(text: str, dataset: str | None, home: str) -> str:
    """Replace the private dataset path and the home directory with placeholders."""
    for path, placeholder in ((dataset, "$AUDIT_DATASET"), (home, "~")):
        if path and len(path) > 1:
            text = text.replace(os.path.realpath(path), placeholder).replace(path, placeholder)
    return text


def cmd_put(args, repo: Path, run_dir: Path) -> dict:
    text = Path(args.file).read_text(encoding="utf-8") if args.file else sys.stdin.read()
    text = sanitize(text, lib.load_run(run_dir).get("dataset"), str(Path.home()))
    if args.kind == "report":
        _require(text.strip() != "", "report must not be empty")
        _require("/home/" not in text, "report must not contain home paths")
        with lib.run_lock(run_dir):
            lib.atomic_write(run_dir / "report.md", text, once=True)
        return {"ok": True, "path": "report.md"}
    try:
        data = json.loads(text)
    except ValueError as error:
        raise Invalid(f"input is not JSON: {error}") from None
    with lib.run_lock(run_dir):
        run = lib.load_run(run_dir)
        lib.require_not_halted(run)
        return PUTTERS[args.kind](run_dir, data)


def put_raw(run_dir: Path, data: Any) -> dict:
    validate_raw(data)
    known = {b["id"]: b for b in load_batches(run_dir)["batches"]}
    _require(data["batch"] in known and data["lens"] in known[data["batch"]]["lenses"], "unknown batch/lens pair")
    name = f"raw/{data['batch']}-{data['lens']}.json"
    lib.write_json(run_dir / name, {**data, "stored": lib.now()}, once=True)
    return {"ok": True, "path": name, "findings": len(data["findings"])}


def put_dedup(run_dir: Path, data: Any) -> dict:
    validate_dedup(data)
    for group in data["groups"]:
        for ref in [group["keep"], *group["merged"]]:
            _require(ref.startswith(f"{data['batch']}:"), f"{ref} belongs to another batch")
            batch_lens, index = ref.split("#")
            raw = run_dir / "raw" / f"{batch_lens.replace(':', '-')}.json"
            _require(raw.is_file(), f"{ref} refers to a missing raw file")
            _require(int(index) < len(lib.read_json(raw)["findings"]), f"{ref} is out of range")
    name = f"raw/{data['batch']}.dedup.json"
    lib.write_json(run_dir / name, data, once=True)
    return {"ok": True, "path": name, "groups": len(data["groups"])}


def put_tools(run_dir: Path, data: Any) -> dict:
    validate_tools(data)
    lib.write_json(run_dir / "tools" / "summary.json", data, once=True)
    return {"ok": True, "path": "tools/summary.json"}


def _transition(run_dir: Path, data: Any, field: str, before: str, after: str, validate) -> tuple[dict, dict]:
    _object(data, field)
    record = load_record(run_dir, str(data.get("id", "")))
    _require(record["state"] == before, f"{record['id']} is {record['state']}; {field} needs state {before}")
    _require(record.get(field) is None, f"{record['id']} already has a {field}")
    validate(data, record)
    record[field] = {**data, "time": lib.now()}
    record["state"] = after
    record["history"].append({"state": after, "time": lib.now()})
    lib.write_json(record_path(run_dir, record["id"]), record)
    return record, data


def put_refutation(run_dir: Path, data: Any) -> dict:
    record, _ = _transition(run_dir, data, "refutation", "candidate", "refuted", validate_refutation)
    return {"ok": True, "id": record["id"], "state": "refuted", "dispute": is_dispute(record),
            "judge_role": judge_role(record)}


def put_judgment(run_dir: Path, data: Any) -> dict:
    def validate(payload, record):
        validate_judgment(payload, record)
        if payload["verdict"] == "duplicate":
            load_record(run_dir, payload["duplicate_of"])

    record, _ = _transition(run_dir, data, "judgment", "refuted", "judged", validate)
    return {"ok": True, "id": record["id"], "state": "judged", "verdict": record["judgment"]["verdict"]}


def put_plan(run_dir: Path, data: Any) -> dict:
    validate_plan(data)
    blocks = lib.load_reference("qa-blocks.json")["blocks"]
    for suite in data["suites"]:
        if suite["id"] in blocks[suite["block"]]["destructive"]:
            _require(suite["destructive"], f"{suite['id']} is destructive in qa-blocks.json")
        if suite["id"] in blocks[suite["block"]]["perf_build"]:
            _require(suite.get("perf_build"), f"{suite['id']} needs perf_build")
    lib.write_json(run_dir / "qa" / "plan.json", data, once=True)
    return {"ok": True, "path": "qa/plan.json", "suites": len(data["suites"])}


def put_qa_suite(run_dir: Path, data: Any) -> dict:
    validate_qa_suite(data)
    plan = lib.read_json(run_dir / "qa" / "plan.json")
    _require(any(s["id"] == data["suite"] and s["block"] == data["block"] for s in plan["suites"]),
             f"{data['suite']} is not planned in block {data['block']}")
    name = f"qa/runs/{data['block']}/{data['suite']}.json"
    lib.write_json(run_dir / name, {**data, "stored": lib.now()}, once=True)
    return {"ok": True, "path": name, "bugs": len(data["bugs"])}


def put_hook_probe(run_dir: Path, data: Any) -> dict:
    d = _object(data, "hook-probe")
    _str_list(d.get("blocked"), "hook-probe.blocked")
    _str_list(d.get("allowed"), "hook-probe.allowed")
    lib.write_json(run_dir / "hook-smoke" / "attempt.json", {**d, "time": lib.now()}, once=True)
    return {"ok": True, "path": "hook-smoke/attempt.json"}


HOOK_PROBES = ("touch-probe", "python-probe")


def hook_check(run_dir: Path, prepare: bool) -> dict:
    """Prepare: an empty hook-smoke/ folder. Evaluate: pass only if the probe agent reached the
    allowed `put hook-probe` (the hook does not over-block) and none of the forbidden probe files
    exists (the hook blocked them)."""
    folder = run_dir / "hook-smoke"
    if prepare:
        folder.mkdir(exist_ok=True)
        for name in (*HOOK_PROBES, "attempt.json"):
            _require(not (folder / name).exists(), f"hook-smoke/{name} already exists; use a new run")
        return {"ok": True, "folder": f"{folder.relative_to(run_dir)}"}
    attempted = (folder / "attempt.json").is_file()
    leaked = [name for name in HOOK_PROBES if (folder / name).exists()]
    log = run_dir / "hook-log.jsonl"
    entries = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines() if line.strip()] if log.is_file() else []
    # Only a block by qa_hook itself counts; a permission denial by the harness would not log here.
    proven = [name for name in HOOK_PROBES
              if any(f"hook-smoke/{name}" in e.get("input", "") and str(e.get("agent_type") or "qa-").startswith("qa-")
                     for e in entries)]
    status = "pass" if attempted and not leaked and len(proven) == len(HOOK_PROBES) else "fail"
    result = {"status": status, "attempted": attempted, "leaked": leaked, "hook_blocked": proven, "time": lib.now()}
    lib.update_run(run_dir, lambda run: run.__setitem__("hook_smoke", result))
    return {"ok": status == "pass", **result}


def put_triage(run_dir: Path, data: Any) -> dict:
    validate_triage(data)
    name = f"qa/runs/{data['block']}/triage.json"
    lib.write_json(run_dir / name, data, once=True)
    return {"ok": True, "path": name, "new_candidates": len(data.get("new_candidates", []))}


def put_qa_dedup(run_dir: Path, data: Any) -> dict:
    validate_qa_dedup(data)
    known = {ref for ref, *_ in qa_bug_refs(run_dir)}
    for group in data["groups"]:
        for ref in [group["keep"], *group["merged"]]:
            _require(ref in known, f"{ref} is not a known QA bug reference")
    lib.write_json(run_dir / "qa" / "dedup.json", data, once=True)
    return {"ok": True, "path": "qa/dedup.json", "groups": len(data["groups"])}


PUTTERS = {
    "raw": put_raw,
    "dedup": put_dedup,
    "tools": put_tools,
    "refutation": put_refutation,
    "judgment": put_judgment,
    "plan": put_plan,
    "qa-suite": put_qa_suite,
    "triage": put_triage,
    "qa-dedup": put_qa_dedup,
    "hook-probe": put_hook_probe,
}


# ---------------------------------------------------------------- assign-ids


def _merge_map(groups: list[dict]) -> dict[str, str]:
    mapping = {}
    for group in groups:
        for ref in group["merged"]:
            mapping[ref] = group["keep"]
    return mapping


def assign_static(repo: Path, run_dir: Path) -> dict:
    counts = {"created": 0, "merged": 0, "unchanged": 0, "batches": 0, "waiting": []}
    config = load_batches(run_dir)
    complete_required = True
    for batch in config["batches"]:
        raws = {lens: run_dir / "raw" / f"{batch['id']}-{lens}.json" for lens in batch["lenses"]}
        if not all(path.is_file() for path in raws.values()):
            if any(path.is_file() for path in raws.values()) or not batch.get("optional"):
                counts["waiting"].append(batch["id"])
            if not batch.get("optional"):
                complete_required = False
            continue
        dedup_path = run_dir / "raw" / f"{batch['id']}.dedup.json"
        if len(raws) > 1 and not dedup_path.is_file():
            counts["waiting"].append(batch["id"])
            complete_required = False
            continue
        merge = _merge_map(lib.read_json(dedup_path)["groups"]) if dedup_path.is_file() else {}
        groups: dict[str, list[tuple[str, dict]]] = {}
        for lens, path in raws.items():
            for index, finding in enumerate(lib.read_json(path)["findings"]):
                ref = f"{batch['id']}:{lens}#{index}"
                groups.setdefault(merge.get(ref, ref), []).append((ref, finding))
        for keep, members in groups.items():
            members.sort(key=lambda item: item[0] != keep)
            finding = {**members[0][1], "lens": members[0][0].split(":")[1].split("#")[0]}
            outcome = upsert_record(run_dir, static_id(finding), "static", finding,
                                    [ref for ref, _ in members], {"batch": batch["id"]})
            counts[outcome] += 1
        counts["batches"] += 1
    tools = run_dir / "tools" / "summary.json"
    if tools.is_file():
        for result in lib.read_json(tools)["results"]:
            for index, failure in enumerate(result.get("failures", [])):
                finding = {**failure, "origin": "tool", "lens": f"tool:{result['name']}"}
                outcome = upsert_record(run_dir, static_id(finding), "static", finding,
                                        [f"tool:{result['name']}#{index}"], {"batch": "tools"})
                counts[outcome] += 1
    # Recon candidates join only after every required finder finished, so finders stay blind.
    if complete_required:
        for index, finding in enumerate(lib.load_reference("recon-candidates.json")["findings"]):
            validate_static_finding(finding)
            finding = {**finding, "lens": "recon"}
            outcome = upsert_record(run_dir, static_id(finding), "static", finding,
                                    [f"recon#{index}"], {"batch": "recon"})
            counts[outcome] += 1
        counts["recon_added"] = True
    return counts


def assign_qa(run_dir: Path) -> dict:
    counts = {"created": 0, "merged": 0, "unchanged": 0}
    refs = qa_bug_refs(run_dir)
    dedup_path = run_dir / "qa" / "dedup.json"
    if refs and not dedup_path.is_file():
        return {**counts, "waiting": "qa/dedup.json"}
    merge = _merge_map(lib.read_json(dedup_path)["groups"]) if dedup_path.is_file() else {}
    by_ref = {ref: (block, suite, bug) for ref, block, suite, bug in refs}
    groups: dict[str, list[str]] = {}
    for ref in by_ref:
        groups.setdefault(merge.get(ref, ref), []).append(ref)
    for keep, members in groups.items():
        members.sort(key=lambda ref: ref != keep)
        block, suite, bug = by_ref[members[0]]
        finding = {**bug, "block": block, "suite": suite, "origin": "qa"}
        outcome = upsert_record(run_dir, qa_id(block, suite, bug["title"]), "qa", finding, members, {})
        counts[outcome] += 1
    for failure in sweep_failures(run_dir):
        title = f"E2E spec {failure['item']} failed"
        finding = {
            "title": title,
            "area": "e2e-sweep",
            "steps": [failure.get("command", "see sweep log")],
            "expected": "The spec passes on a clean build.",
            "expected_ref": "docs/development/testing.md",
            "actual": failure.get("summary", "failed"),
            "evidence": [failure.get("log", "qa/sweep")],
            "frequency": "1/1",
            "priority": "P2",
            "category": "bug",
            "suspect": None,
            "block": "sweep",
            "suite": failure["item"],
            "origin": "sweep",
        }
        outcome = upsert_record(run_dir, qa_id("sweep", failure["item"], title), "qa", finding,
                                [f"sweep:{failure['item']}"], {})
        counts[outcome] += 1
    return counts


# ---------------------------------------------------------------- dup-index and status


def dup_index(run_dir: Path) -> dict:
    index: dict[str, list[dict]] = {}
    for record in all_records(run_dir, "static"):
        finding = record["finding"]
        index.setdefault(finding["file"], []).append(
            {"id": record["id"], "line": finding["line"], "title": finding["title"]})
    clusters = []
    for file, entries in sorted(index.items()):
        entries.sort(key=lambda entry: entry["line"])
        current: list[dict] = []
        for entry in entries:
            if current and entry["line"] - current[-1]["line"] > 15:
                if len(current) > 1:
                    clusters.append({"file": file, "records": current})
                current = []
            current.append(entry)
        if len(current) > 1:
            clusters.append({"file": file, "records": current})
    qa_titles: dict[str, list[str]] = {}
    for record in all_records(run_dir, "qa"):
        qa_titles.setdefault(record["finding"].get("area", "?"), []).append(f"{record['id']}: {record['finding']['title']}")
    result = {"generated": lib.now(), "by_file": index, "near_line_clusters": clusters, "qa_by_area": qa_titles}
    lib.write_json(run_dir / "dup-index.json", result)
    return {"files": len(index), "clusters": len(clusters)}


def _final_priority(record: dict) -> str:
    return (record.get("judgment") or {}).get("priority") or record["finding"]["priority"]


def _findings_markdown(records: list[dict], title: str) -> str:
    lines = [f"# {title}", "", f"Generated {lib.now()}. Only judged records appear here.", ""]
    judged = [r for r in records if r["state"] == "judged"]
    for verdict in ("confirmed", "unproven", "known", "not-reproducible", "duplicate", "rejected"):
        group = [r for r in judged if r["judgment"]["verdict"] == verdict]
        if not group:
            continue
        lines += [f"## {verdict} ({len(group)})", ""]
        for record in sorted(group, key=lambda r: (severity(_final_priority(r)), r["judgment"]["category"], r["id"])):
            judgment, finding = record["judgment"], record["finding"]
            where = f"`{finding['file']}:{finding['line']}`" if "file" in finding else f"{finding.get('block')}/{finding.get('suite')}"
            lines.append(f"- **{record['id']}** {_final_priority(record)} {judgment['category']} — {finding['title']} ({where})")
            if verdict == "duplicate":
                lines.append(f"  - duplicate of {judgment['duplicate_of']}")
            elif verdict == "known":
                lines.append(f"  - known: {judgment['known_source']}")
            elif verdict == "unproven":
                lines.append(f"  - missing fact: {judgment['missing_fact']}")
            lines.append(f"  - {judgment['reasoning'].splitlines()[0][:400]}")
        lines.append("")
    leads = [r for r in records if (r.get("refutation") or {}).get("new_issue")]
    if leads:
        lines += ["## Unverified leads from refuters", ""]
        lines += [f"- from {r['id']}: {json.dumps(r['refutation']['new_issue'], ensure_ascii=False)[:400]}" for r in leads]
        lines.append("")
    return "\n".join(lines)


def status(repo: Path, run_dir: Path) -> dict:
    run = lib.load_run(run_dir)
    static_records = all_records(run_dir, "static")
    qa_records = all_records(run_dir, "qa")

    def tally(records: list[dict]) -> dict:
        states: dict[str, int] = {}
        verdicts: dict[str, int] = {}
        for record in records:
            states[record["state"]] = states.get(record["state"], 0) + 1
            if record.get("judgment"):
                verdict = record["judgment"]["verdict"]
                verdicts[verdict] = verdicts.get(verdict, 0) + 1
        return {"total": len(records), "states": states, "verdicts": verdicts}

    finders = finder_items(repo, run_dir, include_optional=False)
    plan = run_dir / "qa" / "plan.json"
    suites = lib.read_json(plan)["suites"] if plan.is_file() else []
    done_suites = [s["id"] for s in suites if (run_dir / "qa" / "runs" / s["block"] / f"{s['id']}.json").is_file()]
    summary = {
        "run": run_dir.name,
        "baseline": run.get("baseline", {}).get("head"),
        "halted": run.get("halted"),
        "active_session": {k: v for k, v in (run.get("active_session") or {}).items() if k in ("id", "block", "status")} or None,
        "smoke": {k: v.get("status") for k, v in (run.get("smoke") or {}).items()},
        "hook_smoke": (run.get("hook_smoke") or {}).get("status"),
        "tools": (run_dir / "tools" / "summary.json").is_file(),
        "finders": {"done": sum(1 for f in finders if f["done"]), "total": len(finders)},
        "static": tally(static_records),
        "qa_plan": bool(suites),
        "qa_suites": {"done": len(done_suites), "total": len(suites)},
        "sweep": (run_dir / "qa" / "sweep" / "summary.json").is_file(),
        "qa": tally(qa_records),
        "report": (run_dir / "report.md").is_file(),
    }
    lines = [
        f"# Audit run {run_dir.name}",
        "",
        f"Updated {lib.now()}. Baseline `{summary['baseline']}`.",
        "",
        f"- Halted: {'**YES** — ' + run['halted']['reason'] if run.get('halted') else 'no'}",
        f"- Active QA session: {summary['active_session'] or 'none'}",
        f"- Smoke: {json.dumps(summary['smoke'])}; QA hook smoke: {summary['hook_smoke'] or 'not run'}",
        f"- Tools: {'done' if summary['tools'] else 'pending'}",
        f"- Finders: {summary['finders']['done']}/{summary['finders']['total']}",
        f"- Static records: {json.dumps(summary['static'])}",
        f"- Sweep: {'done' if summary['sweep'] else 'pending'}; QA plan: {'yes' if suites else 'no'}",
        f"- QA suites: {summary['qa_suites']['done']}/{summary['qa_suites']['total']}",
        f"- QA records: {json.dumps(summary['qa'])}",
        f"- Report: {'written' if summary['report'] else 'pending'}",
        "",
        "Per-stage detail: `static-findings.md`, `qa-findings.md`, `records/`, `qa/`.",
        "",
    ]
    lib.atomic_write(run_dir / "STATUS.md", "\n".join(lines))
    lib.atomic_write(run_dir / "static-findings.md", _findings_markdown(static_records, "Static findings"))
    lib.atomic_write(run_dir / "qa-findings.md", _findings_markdown(qa_records, "QA findings"))
    return summary


# ---------------------------------------------------------------- CLI


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--run")
    sub = parser.add_subparsers(dest="command", required=True)
    pending = sub.add_parser("pending")
    pending.add_argument("--ids", required=True, choices=STAGES)
    pending.add_argument("--block", choices=BLOCKS)
    pending.add_argument("--models")
    pending.add_argument("--include-optional", action="store_true")
    put = sub.add_parser("put")
    put.add_argument("kind", choices=[*PUTTERS, "report"])
    put.add_argument("--file")
    assign = sub.add_parser("assign-ids")
    assign.add_argument("--kind", required=True, choices=["static", "qa"])
    sub.add_parser("dup-index")
    hook = sub.add_parser("hook-check")
    hook_mode = hook.add_mutually_exclusive_group(required=True)
    hook_mode.add_argument("--prepare", action="store_true")
    hook_mode.add_argument("--evaluate", action="store_true")
    sub.add_parser("status")
    models = sub.add_parser("models")
    models.add_argument("--role")
    models.add_argument("--record")
    models.add_argument("--models")
    args = parser.parse_args(argv)
    try:
        repo = lib.repo_root()
        run_dir = lib.resolve_run(args.run, repo)
        if args.command == "pending":
            result = cmd_pending(args, repo, run_dir)
        elif args.command == "put":
            result = cmd_put(args, repo, run_dir)
        elif args.command == "assign-ids":
            with lib.run_lock(run_dir):
                lib.require_not_halted(lib.load_run(run_dir))
                result = assign_static(repo, run_dir) if args.kind == "static" else assign_qa(run_dir)
        elif args.command == "hook-check":
            result = hook_check(run_dir, args.prepare)
        elif args.command == "dup-index":
            with lib.run_lock(run_dir):
                result = dup_index(run_dir)
        elif args.command == "status":
            with lib.run_lock(run_dir):
                result = status(repo, run_dir)
        elif args.command == "models":
            resolved = resolve_models(args.models, lib.load_run(run_dir).get("model_overrides"))
            if args.record:
                record = load_record(run_dir, args.record)
                role = judge_role(record) if record["state"] == "refuted" else refuter_role(record)
                result = {"role": role, **resolved[role]}
            elif args.role:
                _require(args.role in resolved, f"unknown role {args.role}")
                result = {"role": args.role, **resolved[args.role]}
            else:
                result = resolved
    except lib.AuditError as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
