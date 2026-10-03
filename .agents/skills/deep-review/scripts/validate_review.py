#!/usr/bin/env python3
"""Validate deep-review records, provenance, coverage, source freshness and ignoring."""

import argparse
import json
from pathlib import Path
import subprocess

from init_review import snapshot


def validate(folder, root):
    errors = []
    folder, root = folder.resolve(), root.resolve()

    def check(condition, message):
        if not condition:
            errors.append(message)

    def record(path):
        return json.loads(path.read_text())

    def ref(name):
        path = folder / name
        check(bool(name) and path.resolve().is_relative_to(folder) and path.is_file(),
              f"Missing or unsafe evidence reference: {name}")
        return path

    def required(obj, keys, label):
        for key in keys.split():
            check(key in obj, f"{label}: missing {key}")

    try:
        check(folder.parent == root / "raports", "Run must be directly inside repository raports/")
        for name in ("report.md", "context.md", "run.json", "findings.json"):
            ref(name)
        for name in ("agents", "verification"):
            check((folder / name).is_dir(), f"Missing directory: {name}")
        generated = [p.relative_to(root).as_posix() for p in folder.rglob("*") if p.is_file()]
        generated.append("raports/.deep-review-ignore-probe/nested/evidence.json")
        ignored = subprocess.run(["git", "-C", str(root), "check-ignore", "--stdin", "-z"],
                                 input="\0".join(generated) + "\0", capture_output=True, text=True)
        check(set(generated) <= set(ignored.stdout.split("\0")), "Generated artifacts are not all Git-ignored")
        run, findings = record(folder / "run.json"), record(folder / "findings.json")
        required(run, "schema_version started_at status scope baseline coordinator model_availability runtime_owner assignments coverage handoffs source_changes limitations", "run")
        check(run["schema_version"] == 1, "Unknown schema version")
        check(run["status"] in ("partial", "complete"), "Invalid run status")
        complete = run["status"] == "complete"
        check(bool(run["coordinator"]["agent_id"]), "Coordinator identity required")
        required(run["coordinator"], "observed_model", "coordinator")
        required(run["model_availability"], "source available substitutions", "model availability")
        check(bool(run["model_availability"]["source"]), "Record model availability source")
        if not complete:
            check(bool(run["limitations"]), "Partial review needs explicit limitations")
        scope, baseline = run["scope"], run["baseline"]
        required(scope, "request mode targets included supporting exclusions no_changes", "scope")
        required(baseline, "head comparison_base staged unstaged untracked hashes", "baseline")
        hashes = baseline["hashes"]
        check(set(scope["included"] + scope["supporting"]) <= set(hashes), "Scope files missing baseline hashes")
        current = snapshot(root, hashes)
        changed = {p for p in hashes if current[p] != hashes[p]}
        check(changed <= set(run["source_changes"]), "Source changes not recorded")
        assignments = {}
        original = {}
        for item in run["assignments"]:
            required(item, "id role agent_id perspective areas status requested_model selected_model reasoning_effort observed_model notes findings collaboration limitations", "assignment")
            aid = item["id"]
            check(aid not in assignments, f"Duplicate assignment: {aid}")
            assignments[aid] = item
            check(item["role"] in ("discovery", "review", "verification", "runtime", "intake"), f"Invalid assignment role: {aid}")
            check(item["status"] in ("pending", "running", "complete", "blocked", "not-applicable"), f"Invalid assignment status: {aid}")
            if complete:
                check(item["status"] in ("complete", "not-applicable"), f"Incomplete assignment: {aid}")
            if item["status"] != "complete":
                check(bool(item["limitations"]), f"Unfinished/excluded assignment needs reason: {aid}")
            if item["status"] == "complete":
                check(bool(item["agent_id"]), f"Missing agent identity: {aid}")
            if item["status"] == "complete" and item["role"] != "intake":
                check(bool(item["requested_model"]) and bool(item["selected_model"]), f"Missing model request: {aid}")
                check(item["reasoning_effort"] == "high", f"Expected explicit high reasoning: {aid}")
            for key in ("notes", "findings", "collaboration"):
                if item[key] is None and item["status"] != "complete":
                    continue
                check(item[key].startswith(f"agents/{aid}/"), f"Assignment ownership mismatch: {aid}/{key}")
                if item["status"] == "complete" or (folder / item[key]).exists():
                    ref(item[key])
            if not item["findings"] or not (folder / item["findings"]).is_file():
                continue
            for claim in record(ref(item["findings"])):
                required(claim, "id title location trigger impact priority evidence counterevidence suggested_correction classification change_origin", aid)
                check(claim["id"] not in original, f"Duplicate original ID: {claim['id']}")
                original[claim["id"]] = item["agent_id"]
                for name in claim["evidence"]:
                    ref(name)
        identities = {run["coordinator"]["agent_id"]} | {a["agent_id"] for a in assignments.values() if a["agent_id"]}
        if run["runtime_owner"] is not None:
            check(run["runtime_owner"] in identities, "Unknown runtime owner")
        covered = set()
        area_ids = set()
        for area in run["coverage"]:
            required(area, "id paths assignments", "coverage")
            check(area["id"] not in area_ids, f"Duplicate coverage area: {area['id']}")
            area_ids.add(area["id"])
            covered.update(area["paths"])
            reviewers = []
            for aid in area["assignments"]:
                check(aid in assignments, f"Unknown coverage assignment: {aid}")
                if aid in assignments:
                    item = assignments[aid]
                    check(area["id"] in item["areas"], f"Coverage not acknowledged by {aid}")
                    if item["role"] == "review" and item["status"] == "complete":
                        reviewers.append(item)
            if complete:
                check(len({a["agent_id"] for a in reviewers}) >= 2 and
                      len({a["perspective"] for a in reviewers}) >= 2,
                      f"Area lacks two independent perspectives: {area['id']}")
        if complete and not scope["no_changes"]:
            check(bool(scope["included"]), "Empty scope cannot claim a substantive completed review")
            check(set(scope["included"]) <= covered, "Included files lack coverage")
        if scope["no_changes"]:
            check(scope["mode"] == "changes" and bool(baseline["head"]) and
                  not any(baseline[k] for k in ("staged", "unstaged", "untracked")) and
                  not scope["included"] and not findings["candidates"], "Invalid no-changes claim")
            commits = []
            for field in ("comparison_base", "head"):
                revision = baseline.get(field)
                message = f"Cannot verify no-changes claim: invalid or unavailable {field}"
                if not isinstance(revision, str) or not revision:
                    check(False, message)
                    continue
                resolved = subprocess.run(
                    ["git", "-C", str(root), "rev-parse", "--verify", "--end-of-options",
                     f"{revision}^{{commit}}"], capture_output=True, text=True)
                check(resolved.returncode == 0, message)
                if resolved.returncode == 0:
                    commits.append(resolved.stdout.strip())
            if len(commits) == 2:
                # Compare the recorded endpoints, not the currently checked-out HEAD.
                diff = subprocess.run(
                    ["git", "-C", str(root), "diff", "--quiet", "--no-ext-diff",
                     "--no-textconv", "--ignore-submodules=none", *commits, "--"],
                    capture_output=True, text=True)
                check(diff.returncode != 1,
                      "Invalid no-changes claim: recorded commit range has changes")
                check(diff.returncode in (0, 1),
                      "Cannot verify no-changes claim: Git could not compare recorded commits")
        handoff_ids = set()
        for handoff in run["handoffs"]:
            required(handoff, "id sender recipient question evidence response status", "handoff")
            check(handoff["id"] not in handoff_ids, "Duplicate handoff ID")
            handoff_ids.add(handoff["id"])
            check(handoff["sender"] in identities and handoff["recipient"] in identities, "Unknown handoff identity")
            check(handoff["status"] in ("answered", "blocked", "pending"), "Invalid handoff status")
            check(bool(handoff["question"]), "Empty handoff question")
            ref(handoff["evidence"])
            if handoff["status"] == "answered":
                ref(handoff["response"])
            if complete:
                check(handoff["status"] == "answered", "Unanswered handoff in complete review")
        if complete and not scope["no_changes"]:
            check(bool(run["handoffs"]), "Missing collaboration round")
        canonical_ids, aliases, expected_duplicates = set(), set(), set()
        for claim in findings["candidates"]:
            required(claim, "id original_ids originators title location trigger impact priority evidence counterevidence verdict verification stale suggested_correction classification change_origin", "candidate")
            cid = claim["id"]
            check(cid not in canonical_ids, f"Duplicate canonical ID: {cid}")
            canonical_ids.add(cid)
            check(bool(claim["original_ids"]), f"Missing originals: {cid}")
            check(not (aliases & set(claim["original_ids"])), f"Original mapped twice: {cid}")
            aliases.update(claim["original_ids"])
            check(set(claim["originators"]) == {original.get(x) for x in claim["original_ids"]}, f"Originator mismatch: {cid}")
            if len(claim["original_ids"]) > 1:
                expected_duplicates.update((x, cid) for x in claim["original_ids"])
            check(claim["priority"] in ("P0", "P1", "P2", "P3"), f"Invalid priority: {cid}")
            check(claim["classification"] in ("defect", "design-risk"), f"Invalid classification: {cid}")
            check(claim["change_origin"] in ("introduced", "pre-existing", "unknown"), f"Invalid change origin: {cid}")
            check(claim["verdict"] in ("confirmed", "rejected", "unresolved"), f"Invalid verdict: {cid}")
            required(claim["location"], "path line", cid)
            check(claim["location"]["path"] in hashes and isinstance(claim["location"]["line"], int) and claim["location"]["line"] > 0, f"Invalid source location: {cid}")
            check(all(claim[k] for k in ("title", "trigger", "impact", "evidence", "suggested_correction")), f"Missing candidate substance: {cid}")
            for name in claim["evidence"]:
                ref(name)
            if not claim["verification"]:
                check(not complete and claim["verdict"] == "unresolved", f"Missing independent verification: {cid}")
                continue
            verification = record(ref(claim["verification"]))
            required(verification, "candidate_id agent_id verdict evidence counterevidence basis commands limitations verified_hashes", "verification")
            check(verification["candidate_id"] == cid and verification["verdict"] == claim["verdict"], f"Verdict mismatch: {cid}")
            vid = verification["agent_id"]
            check(vid in identities and vid not in claim["originators"], f"Invalid independent verifier: {cid}")
            check(any(a["agent_id"] == vid and a["role"] == "verification" and a["status"] == "complete" for a in assignments.values()), f"Missing completed verification assignment: {cid}")
            check(verification["basis"] in ("executed", "code-and-contract", "both", "unavailable"), f"Invalid evidence basis: {cid}")
            check(bool(verification["evidence"]), f"No verification evidence: {cid}")
            for name in verification["evidence"]:
                ref(name)
            verified = verification["verified_hashes"]
            check(claim["location"]["path"] in verified, f"Location not verified: {cid}")
            check(set(verified) <= set(hashes), f"Verified dependency absent from baseline: {cid}")
            fresh = snapshot(root, verified) == verified
            check(fresh or (claim["stale"] and claim["verdict"] == "unresolved"), f"Stale finding needs recheck or unresolved verdict: {cid}")
            if claim["stale"]:
                check(claim["verdict"] == "unresolved", f"Stale candidate cannot be decided: {cid}")
            if verification["basis"] in ("executed", "both"):
                check(bool(verification["commands"]), f"Executed verdict has no commands: {cid}")
            for command in verification["commands"]:
                required(command, "command cwd exit_code observation", "command")
            if claim["verdict"] == "unresolved":
                check(bool(verification["limitations"]), f"Unresolved candidate needs reason: {cid}")
            else:
                check(verification["basis"] != "unavailable", f"Decided claim has unavailable evidence: {cid}")
        check(aliases == set(original), "Original findings lost or unknown aliases present")
        actual_duplicates = [(x["original_id"], x["canonical_id"]) for x in findings["duplicates"]]
        check(len(actual_duplicates) == len(set(actual_duplicates)) and set(actual_duplicates) == expected_duplicates,
              "Duplicate mappings do not match merged originals")
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        errors.append(f"Malformed or unreadable record: {error}")
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    args = parser.parse_args()
    errors = validate(args.folder, Path.cwd())
    for error in errors:
        print(f"ERROR: {error}")
    if not errors:
        print("Valid review record. See run.json for complete/partial status and limitations.")
    raise SystemExit(bool(errors))


if __name__ == "__main__":
    main()
