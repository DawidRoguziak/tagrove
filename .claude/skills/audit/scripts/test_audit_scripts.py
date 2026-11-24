"""Unit tests for the /audit scripts. Synthetic temporary trees only; no app session, no dataset.

python3 -m unittest discover .claude/skills/audit/scripts
"""
from __future__ import annotations

import contextlib
import io
import json
import os
import re
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import audit_lib as lib  # noqa: E402
import audit_state as state  # noqa: E402
import dataset_guard  # noqa: E402
import qa_fs  # noqa: E402
import qa_hook  # noqa: E402
import qa_session  # noqa: E402
import run_e2e_sweep  # noqa: E402

REPO = HERE.parents[3]
RUN = "20261108-1200-test"


def finding(title="Lock held across await", file="src-tauri/src/a.rs", line=10, priority="P2", **extra):
    return {"title": title, "file": file, "line": line, "category": "bug", "priority": priority,
            "origin": "pre-existing", "trigger": "t", "mechanism": "m", "impact": "i",
            "evidence": [f"{file}:{line}"], **extra}


def bug(title="Tile stays blank", priority="P2", category="bug"):
    return {"title": title, "area": "gallery", "steps": ["open"], "expected": "x", "expected_ref": None,
            "actual": "y", "evidence": ["screen-0001.png"], "frequency": "2/2", "priority": priority,
            "category": category, "suspect": None}


class FakeRepo(unittest.TestCase):
    """A minimal repository with one run directory; cwd is switched into it."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="audit-test-"))
        self.repo = self.tmp / "repo"
        (self.repo / ".git").mkdir(parents=True)
        (self.repo / "src-tauri" / "src").mkdir(parents=True)
        (self.repo / "src" / "x").mkdir(parents=True)
        (self.repo / "src-tauri" / "src" / "a.rs").write_text("fn a() {}\n")
        (self.repo / "src" / "x" / "b.ts").write_text("export {}\n")
        (self.repo / "src" / "x" / "b.test.ts").write_text("test\n")
        self.run_dir = self.repo / "docs" / "review" / RUN
        (self.run_dir / "context").mkdir(parents=True)
        (self.repo / "docs" / "review" / "LATEST").write_text(RUN + "\n")
        self.dataset = self.tmp / "dataset"
        (self.dataset / "for-claude-tests").mkdir(parents=True)
        lib.write_json(self.run_dir / "run.json", {
            "schema": 1, "run": RUN, "dataset": str(self.dataset),
            "qa_tmpdir": str(self.dataset / "for-claude-tests" / "q"),
            "halted": None, "active_session": None, "sessions": [], "model_overrides": {}})
        lib.write_json(self.run_dir / "context" / "batches.json", {
            "lens_agents": {"rust": "review-rust", "frontend": "review-frontend", "conventions": "review-conventions",
                            "tooling": "review-tooling"},
            "batches": [
                {"id": "B1", "title": "one", "critical": True, "globs": ["src-tauri/src/*.rs", "src/x/*"],
                 "lenses": ["rust", "conventions"]},
                {"id": "B2", "title": "two", "critical": False, "globs": ["src/x/*"], "lenses": ["frontend"]},
                {"id": "B9", "title": "optional", "optional": True, "globs": ["src/x/*"], "lenses": ["tooling"]},
            ]})
        self.old_cwd = os.getcwd()
        os.chdir(self.repo)

    def tearDown(self):
        os.chdir(self.old_cwd)
        shutil.rmtree(self.tmp, ignore_errors=True)

    def cli(self, *argv, stdin=None):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err), \
                mock.patch("sys.stdin", io.StringIO(stdin or "")):
            code = state.main(list(argv))
        return code, json.loads(out.getvalue() or err.getvalue() or "{}")

    def put(self, kind, data):
        return self.cli("put", kind, stdin=json.dumps(data))


class ModelTests(unittest.TestCase):
    def test_defaults_and_overrides(self):
        models = state.resolve_models()
        self.assertEqual(models["finder_critical"], {"model": "fable", "effort": "high"})
        self.assertEqual(models["clerk"], {"model": "haiku", "effort": "low"})
        changed = state.resolve_models("finder=sonnet:medium,judge_high=opus", {"clerk": {"model": "sonnet"}})
        self.assertEqual(changed["finder"], {"model": "sonnet", "effort": "medium"})
        self.assertEqual(changed["judge_high"], {"model": "opus", "effort": "high"})
        self.assertEqual(changed["clerk"]["model"], "sonnet")
        for bad in ("nobody=opus", "finder=gpt", "finder=opus:extreme", "finder"):
            with self.assertRaises(state.Invalid):
                state.resolve_models(bad)

    def test_every_role_in_batches_and_blocks_exists(self):
        roles = lib.load_reference("models.json")["roles"]
        config = lib.load_reference("batches.json")
        for batch in config["batches"]:
            for lens in batch["lenses"]:
                self.assertIn(state.lens_role(batch, lens), roles)
                self.assertIn(lens, config["lens_agents"])
        for block in lib.load_reference("qa-blocks.json")["blocks"].values():
            self.assertIn(block["tester_role"], roles)

    def test_role_resolution(self):
        record = {"kind": "static", "finding": finding(priority="P1"), "refutation": None}
        self.assertEqual(state.refuter_role(record), "static_refuter_high")
        record["finding"]["priority"] = "P3"
        self.assertEqual(state.refuter_role(record), "static_refuter_low")
        record["refutation"] = {"verdict": "confirmed", "priority": "P3"}
        self.assertEqual(state.judge_role(record), "judge_low")
        record["refutation"] = {"verdict": "confirmed", "priority": "P1"}
        self.assertEqual(state.judge_role(record), "judge_high", "a 2-level change is a dispute and P1")
        record["refutation"] = {"verdict": "uncertain", "priority": "P3", "missing_fact": "x"}
        self.assertEqual(state.judge_role(record), "judge_high")
        record["finding"]["priority"] = "P1"
        record["refutation"] = {"verdict": "rejected", "priority": "P2"}
        self.assertTrue(state.is_dispute(record))
        qa = {"kind": "qa", "finding": {**bug(), "block": "S1"}}
        self.assertEqual(state.refuter_role(qa), "qa_refuter")
        qa["finding"]["category"] = "data-safety"
        self.assertEqual(state.refuter_role(qa), "qa_refuter_safety")
        qa["finding"].update(category="bug", block="S2")
        self.assertEqual(state.refuter_role(qa), "qa_refuter_safety")

    def test_batches_reference_existing_files(self):
        config = lib.load_reference("batches.json")
        ids = [batch["id"] for batch in config["batches"]]
        self.assertEqual(len(ids), len(set(ids)))
        for batch in config["batches"]:
            self.assertTrue(state.batch_files(REPO, batch), f"{batch['id']} matches no files")

    def test_every_batch_glob_matches_and_every_backend_file_is_batched(self):
        config = lib.load_reference("batches.json")
        batched = set()
        for batch in config["batches"]:
            for pattern in batch["globs"]:
                matched = state.batch_files(REPO, {**batch, "globs": [pattern], "tests": True})
                self.assertTrue(matched, f"{batch['id']}: {pattern!r} matches no file")
            batched.update(state.batch_files(REPO, {**batch, "tests": True}))
        sources = [
            REPO / "src-tauri/src",
            REPO / "src-tauri/crates/media_tagger_core/src",
            # The GPUI port: both crates of the workspace src-gpui/.
            REPO / "src-gpui/crates/media_tagger_client/src",
            REPO / "src-gpui/crates/media_tagger_gpui/src",
        ]
        for root in sources:
            self.assertTrue(root.is_dir(), f"{root} is not a source directory")
        unbatched = sorted(
            path.relative_to(REPO).as_posix()
            for root in sources
            for path in root.rglob("*.rs")
            if path.name != "mod.rs" and path.relative_to(REPO).as_posix() not in batched
        )
        self.assertEqual(unbatched, [], "Rust sources outside every audit batch")

    def test_gpui_sources_are_audited_by_default_with_the_gpui_lens(self):
        config = lib.load_reference("batches.json")
        self.assertEqual(config["lens_agents"]["gpui"], "review-gpui")
        default = [batch for batch in config["batches"] if not batch.get("optional")]
        covered = set()
        for batch in default:
            covered.update(state.batch_files(REPO, batch))
        adapter = sorted(
            path.relative_to(REPO).as_posix()
            for path in (REPO / "src-gpui/crates/media_tagger_gpui/src").rglob("*.rs")
        )
        self.assertTrue(adapter)
        self.assertEqual([path for path in adapter if path not in covered], [],
                         "adapter sources only in optional batches")
        with_lens = set()
        for batch in default:
            if "gpui" in batch["lenses"]:
                with_lens.update(state.batch_files(REPO, batch))
        self.assertEqual([path for path in adapter if path not in with_lens], [],
                         "adapter sources without the gpui lens")
        # The integration tests of both crates are an optional batch, like the Tauri ones.
        optional = [batch for batch in config["batches"] if batch.get("optional")]
        tests = set()
        for batch in optional:
            tests.update(state.batch_files(REPO, batch))
        self.assertIn("src-gpui/crates/media_tagger_gpui/tests/gallery_grid.rs", tests)
        self.assertIn("src-gpui/crates/media_tagger_client/tests/port_contract.rs", tests)

    def test_audit_readme_states_the_batch_and_finder_counts_of_batches_json(self):
        config = lib.load_reference("batches.json")
        default = [batch for batch in config["batches"] if not batch.get("optional")]
        finders = sum(len(batch["lenses"]) for batch in default)
        readme = (REPO / "docs/development/audit/README.md").read_text(encoding="utf-8")
        stated = re.findall(r"Finders: (\d+)|about (\d+) finders", readme)
        self.assertGreaterEqual(len(stated), 2, "the README no longer states the finder count")
        for exact, about in stated:
            self.assertEqual(int(exact or about), finders)
        self.assertIn(f"({len(config['batches'])} batches;", readme)
        # The sentence naming the batches of the gpui lens lists exactly the batches that carry it.
        sentence = re.search(r"The `gpui` lens \(the `review-gpui` agent\) runs on ([^;.]+)", readme)
        self.assertIsNotNone(sentence, "the README no longer names the batches of the gpui lens")
        named = set()
        for first, last in re.findall(r"(B\d+[a-z]?)(?: to (B\d+[a-z]?))?", sentence.group(1)):
            ids = [batch["id"] for batch in config["batches"]]
            named.update(ids[ids.index(first):ids.index(last or first) + 1])
        self.assertEqual(named, {batch["id"] for batch in config["batches"] if "gpui" in batch["lenses"]})


class StaticFlowTests(FakeRepo):
    def test_pending_put_assign_verify_status(self):
        code, pending = self.cli("pending", "--ids", "static")
        self.assertEqual(code, 0)
        ids = [item["id"] for item in pending["items"]]
        self.assertEqual(ids, ["tools", "B1:rust", "B1:conventions", "B1:dedup", "B2:frontend"])
        rust = pending["items"][1]
        self.assertEqual(rust["role"], "finder_critical")
        self.assertEqual(pending["items"][2]["role"], "finder_rules")
        self.assertEqual(rust["files"], ["src-tauri/src/a.rs", "src/x/b.ts"], "tests are excluded")
        self.assertEqual(pending["models"]["finder_critical"]["model"], "fable")
        code, all_items = self.cli("pending", "--ids", "static", "--include-optional")
        self.assertIn("B9:tooling", [item["id"] for item in all_items["items"]])

        raw = {"batch": "B1", "lens": "rust", "findings": [finding(), finding(title="Other", line=40)],
               "files_reviewed": ["src-tauri/src/a.rs"]}
        self.assertEqual(self.put("raw", raw)[0], 0)
        self.assertEqual(self.put("raw", raw)[0], 2, "raw results are write-once")
        self.assertEqual(self.put("raw", {**raw, "lens": "frontend"})[0], 2, "lens not in batch")
        self.assertEqual(self.put("raw", {**raw, "batch": "B1", "lens": "conventions",
                                          "findings": [finding(file="/srv/x/a.rs")]})[0], 2, "absolute path")
        self.put("raw", {"batch": "B1", "lens": "conventions", "findings": [finding(title="Same bug again")]})
        self.put("raw", {"batch": "B2", "lens": "frontend", "findings": []})

        code, result = self.cli("assign-ids", "--kind", "static")
        self.assertEqual(result["waiting"], ["B1"], "B1 waits for its dedup")
        self.assertNotIn("recon_added", result)
        bad = {"batch": "B1", "groups": [{"keep": "B1:rust#0", "merged": ["B1:conventions#5"], "reason": "x"}]}
        self.assertEqual(self.put("dedup", bad)[0], 2, "out of range reference")
        dedup = {"batch": "B1", "groups": [{"keep": "B1:rust#0", "merged": ["B1:conventions#0"], "reason": "same"}]}
        self.assertEqual(self.put("dedup", dedup)[0], 0)
        code, result = self.cli("assign-ids", "--kind", "static")
        recon = len(lib.load_reference("recon-candidates.json")["findings"])
        self.assertEqual(result["created"], 2 + recon)
        self.assertTrue(result["recon_added"])
        code, again = self.cli("assign-ids", "--kind", "static")
        self.assertEqual(again["created"], 0, "assign-ids is idempotent")

        record_id = state.static_id(finding())
        record = state.load_record(self.run_dir, record_id)
        self.assertEqual(record["sources"], ["B1:rust#0", "B1:conventions#0"])
        self.assertEqual(record["merged_from"], ["B1:conventions#0"])
        self.assertEqual(record["state"], "candidate")
        self.assertEqual(state.static_id(finding(title="Lock  held, across AWAIT!")), record_id, "titles normalize")

        code, verify = self.cli("pending", "--ids", "static-verify")
        item = next(i for i in verify["items"] if i["id"] == record_id)
        self.assertEqual((item["needs"], item["role"]), ("refute", "static_refuter_low"))
        judgment = {"id": record_id, "verdict": "confirmed", "priority": "P2", "category": "bug",
                    "reasoning": "r", "evidence": ["e"]}
        self.assertEqual(self.put("judgment", judgment)[0], 2, "judge before refuter is refused")
        refutation = {"id": record_id, "verdict": "uncertain", "priority": "P2", "reasoning": "r",
                      "evidence": ["e"], "missing_fact": "timing"}
        code, refuted = self.put("refutation", refutation)
        self.assertEqual((code, refuted["judge_role"], refuted["dispute"]), (0, "judge_high", True))
        self.assertEqual(self.put("refutation", refutation)[0], 2, "exactly one refuter")
        self.assertEqual(self.put("judgment", {**judgment, "verdict": "duplicate"})[0], 2, "needs duplicate_of")
        self.assertEqual(self.put("judgment", {**judgment, "verdict": "known"})[0], 2, "needs known_source")
        self.assertEqual(self.put("judgment", {**judgment, "verdict": "not-reproducible"})[0], 2, "QA-only verdict")
        self.assertEqual(self.put("judgment", judgment)[0], 0)
        self.assertEqual(self.put("judgment", judgment)[0], 2, "exactly one judge")
        self.assertEqual(state.load_record(self.run_dir, record_id)["state"], "judged")

        code, summary = self.cli("status")
        self.assertEqual(summary["static"]["verdicts"], {"confirmed": 1})
        self.assertIn(record_id, (self.run_dir / "static-findings.md").read_text())
        self.assertTrue((self.run_dir / "STATUS.md").is_file())
        self.cli("dup-index")
        self.assertTrue((self.run_dir / "dup-index.json").is_file())

    def test_put_replaces_private_paths(self):
        home = str(Path.home())
        raw = {"batch": "B2", "lens": "frontend", "findings": [],
               "leads": [{"title": f"see {self.dataset}/photos and {home}/notes", "file": "x", "line": 1, "note": "n"}]}
        self.assertEqual(self.put("raw", raw)[0], 0)
        text = (self.run_dir / "raw" / "B2-frontend.json").read_text()
        self.assertNotIn(str(self.dataset), text)
        self.assertNotIn(home + "/", text)
        self.assertIn("$AUDIT_DATASET/photos", text)

    def test_halted_run_refuses_pending_and_put(self):
        lib.halt(self.run_dir, "guard dirty", "test")
        self.assertEqual(self.cli("pending", "--ids", "static")[0], 2)
        self.assertEqual(self.put("raw", {"batch": "B2", "lens": "frontend", "findings": []})[0], 2)
        import clear_halt
        with self.assertRaises(SystemExit), contextlib.redirect_stderr(io.StringIO()):
            clear_halt.main([])
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(clear_halt.main(["--confirm-inspected"]), 0)
        self.assertEqual(self.cli("pending", "--ids", "static")[0], 0)

    def test_run_resolution_refuses_paths_outside_review(self):
        with self.assertRaises(lib.AuditError):
            lib.resolve_run("../etc", self.repo)
        with self.assertRaises(lib.AuditError):
            lib.resolve_run(str(self.tmp), self.repo)
        self.assertEqual(lib.resolve_run(None, self.repo), self.run_dir.resolve())


class QaFlowTests(FakeRepo):
    def install_hooks(self):
        agents = self.repo / ".claude" / "agents"
        agents.mkdir(parents=True)
        for name in ("qa-tester", "qa-refuter"):
            (agents / f"{name}.md").write_text(
                "---\nname: x\nhooks:\n  PreToolUse:\n    - hooks:\n        - command: qa_hook.py\n---\nbody\n")
        (self.repo / ".claude" / "settings.local.json").write_text(json.dumps(
            {"hooks": {"PreToolUse": [{"matcher": "*", "hooks": [{"command": "qa_hook.py --scope settings"}]}]}}))
        lib.update_run(self.run_dir, lambda run: run.__setitem__("hook_smoke", {"status": "pass"}))

    def test_qa_stages_need_hooks(self):
        self.assertEqual(self.cli("pending", "--ids", "qa-run")[0], 2)
        self.assertEqual(len(state.hooks_missing(self.repo)), 3)
        self.install_hooks()
        self.assertEqual(state.hooks_missing(self.repo), [])
        lib.update_run(self.run_dir, lambda run: run.__setitem__("hook_smoke", {"status": "fail"}))
        code, error = self.cli("pending", "--ids", "qa-verify")
        self.assertEqual(code, 2)
        self.assertIn("hook smoke", error["error"])

    def test_hook_check(self):
        import subprocess
        self.assertEqual(self.cli("hook-check", "--prepare")[0], 0)
        folder = self.run_dir / "hook-smoke"
        code, result = self.cli("hook-check", "--evaluate")
        self.assertEqual((code, result["status"]), (0, "fail"), "nothing attempted")
        # The real hook, as the probe agent would trigger it, logs both blocks.
        hook = str(HERE / "qa_hook.py")
        for command in (f"touch docs/review/{RUN}/hook-smoke/touch-probe",
                        f"python3 -c \"open('docs/review/{RUN}/hook-smoke/python-probe', 'w')\""):
            blocked = subprocess.run([hook, "--scope", "agent"], input=json.dumps(
                {"tool_name": "Bash", "tool_input": {"command": command}, "cwd": str(self.repo)}),
                capture_output=True, text=True, env={**os.environ, "CLAUDE_PROJECT_DIR": str(self.repo)})
            self.assertEqual(blocked.returncode, 2, command)
        self.assertEqual(self.put("hook-probe", {"blocked": ["touch", "python3 -c"], "allowed": []})[0], 0)
        code, result = self.cli("hook-check", "--evaluate")
        self.assertEqual(result["status"], "pass", result)
        self.assertEqual(lib.load_run(self.run_dir)["hook_smoke"]["status"], "pass")
        (folder / "touch-probe").write_text("leaked")
        code, result = self.cli("hook-check", "--evaluate")
        self.assertEqual((result["status"], result["leaked"]), ("fail", ["touch-probe"]))

    def test_hook_check_needs_hook_log(self):
        self.cli("hook-check", "--prepare")
        self.put("hook-probe", {"blocked": ["touch", "python3 -c"], "allowed": []})
        code, result = self.cli("hook-check", "--evaluate")
        self.assertEqual(result["status"], "fail", "a refusal that the hook did not log proves nothing")

    def test_plan_suites_dedup_assign(self):
        self.install_hooks()
        code, prep = self.cli("pending", "--ids", "qa-prep")
        self.assertEqual([i["id"] for i in prep["items"]], ["sweep", "plan"])
        bad_plan = {"suites": [{"id": "QA-10", "block": "S2", "title": "Delete", "scenario_ref": "x",
                                "destructive": False}]}
        self.assertEqual(self.put("plan", bad_plan)[0], 2, "QA-10 must stay destructive")
        plan = {"suites": [
            {"id": "QA-01", "block": "S1", "title": "Scan", "scenario_ref": "qa-scenarios.md#qa-01", "destructive": False},
            {"id": "QA-09", "block": "S1", "title": "Soak", "scenario_ref": "qa-scenarios.md#qa-09", "destructive": False,
             "perf_build": True},
            {"id": "QA-10", "block": "S2", "title": "Delete", "scenario_ref": "qa-scenarios.md#qa-10", "destructive": True},
            {"id": "QA-T1", "block": "S2", "title": "Target", "scenario_ref": "records", "destructive": True,
             "targets": ["S-0123abcd"]}]}
        self.assertEqual(self.put("plan", plan)[0], 0)
        code, run = self.cli("pending", "--ids", "qa-run")
        s1 = run["items"][0]
        self.assertEqual([[s["id"] for s in session["suites"]] for session in s1["sessions"]], [["QA-01"], ["QA-09"]])
        self.assertEqual([session["perf"] for session in s1["sessions"]], [False, True])
        self.assertEqual(run["items"][1]["tester_role"], "qa_tester_safety")
        code, only = self.cli("pending", "--ids", "qa-run", "--block", "S2")
        self.assertEqual([i["id"] for i in only["items"]], ["S2"])

        session = "c" * 32
        suite = {"block": "S1", "suite": "QA-01", "status": "completed", "session": session, "summary": "s",
                 "bugs": [bug(), bug(title="Counter wrong")]}
        self.assertEqual(self.put("qa-suite", {**suite, "suite": "QA-05"})[0], 2, "unplanned suite")
        self.assertEqual(self.put("qa-suite", {**suite, "bugs": [{**bug(), "frequency": "often"}]})[0], 2)
        self.assertEqual(self.put("qa-suite", suite)[0], 0)
        self.put("qa-suite", {"block": "S2", "suite": "QA-10", "status": "partial", "session": session,
                              "summary": "s", "bugs": [bug(title="Blank tile", category="data-safety")]})
        code, verify = self.cli("pending", "--ids", "qa-verify")
        self.assertEqual(verify["items"][0]["id"], "qa-dedup")
        self.assertEqual(self.cli("assign-ids", "--kind", "qa")[1]["waiting"], "qa/dedup.json")
        groups = {"groups": [{"keep": "S1/QA-01#0", "merged": ["S2/QA-10#0"], "reason": "same blank tile"}]}
        self.assertEqual(self.put("qa-dedup", {"groups": [{"keep": "S1/QA-01#0", "merged": ["S1/QA-01#9"],
                                                           "reason": "x"}]})[0], 2)
        self.assertEqual(self.put("qa-dedup", groups)[0], 0)
        lib.write_json(self.run_dir / "qa" / "sweep" / "about.json",
                       {"item": "about", "status": "failed", "summary": "1 failing", "log": "qa/sweep/about.log"})
        code, assigned = self.cli("assign-ids", "--kind", "qa")
        self.assertEqual(assigned["created"], 3)
        code, verify = self.cli("pending", "--ids", "qa-verify")
        self.assertEqual(len(verify["items"]), 3)
        merged = state.load_record(self.run_dir, state.qa_id("S1", "QA-01", "Tile stays blank"))
        self.assertEqual(merged["sources"], ["S1/QA-01#0", "S2/QA-10#0"])
        refutation = {"id": merged["id"], "verdict": "confirmed", "priority": "P2", "reasoning": "r",
                      "evidence": ["e"], "grounds": "none", "reproduced": "2/2"}
        self.assertEqual(self.put("refutation", refutation)[0], 0)
        self.assertEqual(self.put("judgment", {"id": merged["id"], "verdict": "unproven", "priority": "P2",
                                               "category": "bug", "reasoning": "r", "evidence": ["e"],
                                               "missing_fact": "x"})[0], 2, "unproven is static-only")
        self.assertEqual(self.put("judgment", {"id": merged["id"], "verdict": "not-reproducible", "priority": "P3",
                                               "category": "bug", "reasoning": "r", "evidence": ["e"]})[0], 0)
        triage = {"block": "S1", "assignments": [{"suite": "QA-01", "source": "console", "text": "TypeError"}],
                  "new_candidates": [{**bug(title="Console TypeError"), "suite": "QA-01"}]}
        self.assertEqual(self.put("triage", triage)[0], 0)


class DatasetGuardTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="audit-guard-"))
        (self.root / "photos").mkdir()
        (self.root / "photos" / "a.jpg").write_bytes(b"a")
        (self.root / "for-claude-tests").mkdir()
        os.symlink("photos/a.jpg", self.root / "link")

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def test_detects_changes_outside_writable_folder_only(self):
        before = dataset_guard.snapshot(self.root)
        (self.root / "for-claude-tests" / "copy.jpg").write_bytes(b"x")
        self.assertTrue(dataset_guard.compare(before, dataset_guard.snapshot(self.root))["clean"])
        os.utime(self.root / "photos" / "a.jpg", ns=(1, 1))
        (self.root / "photos" / "new.png").write_bytes(b"n")
        (self.root / "photos" / ".mediatagger-abc.pending").write_bytes(b"")
        result = dataset_guard.compare(before, dataset_guard.snapshot(self.root))
        self.assertFalse(result["clean"])
        self.assertIn("photos/new.png", result["added"])
        self.assertIn("photos/a.jpg", [c["path"] for c in result["changed"]])
        self.assertEqual(result["suspicious"], ["photos/.mediatagger-abc.pending"])

    def test_check_halts_the_run(self):
        repo_case = FakeRepo("setUp")
        repo_case.setUp()
        try:
            manifest = repo_case.run_dir / "context" / "dataset-manifest.json"
            lib.write_json(manifest, dataset_guard.snapshot(self.root))
            self.assertTrue(dataset_guard.check(self.root, manifest, repo_case.run_dir, "clean")["clean"])
            (self.root / "photos" / "a.jpg").write_bytes(b"changed")
            self.assertFalse(dataset_guard.check(self.root, manifest, repo_case.run_dir, "dirty")["clean"])
            self.assertIn("dataset guard", lib.load_run(repo_case.run_dir)["halted"]["reason"])
            self.assertEqual(len(list((repo_case.run_dir / "guard").glob("*.json"))), 2)
        finally:
            repo_case.tearDown()


class QaFsTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(os.path.realpath(tempfile.mkdtemp(prefix="audit-fs-")))
        self.media = self.root / "media"
        (self.media / "dataset" / "photos").mkdir(parents=True)
        (self.media / "dataset" / "photos" / "a.jpg").write_bytes(b"a")
        self.outside = self.root / "outside.txt"
        self.outside.write_text("keep")
        self.ctx = {"media": str(self.media), "dataset": str(self.root / "ds"), "session": "c" * 32}

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def test_paths_must_stay_inside_the_copy(self):
        for bad in ("../outside.txt", str(self.outside), "dataset/../../outside.txt", "/etc/passwd", ".", ""):
            with self.assertRaises(qa_fs.Refused, msg=bad):
                qa_fs.inside(bad, str(self.media))
        os.symlink(self.root, self.media / "escape")
        with self.assertRaises(qa_fs.Refused):
            qa_fs.inside("escape/outside.txt", str(self.media))
        self.assertEqual(qa_fs.inside("dataset/photos/a.jpg", str(self.media)),
                         str(self.media / "dataset" / "photos" / "a.jpg"))

    def test_operations(self):
        qa_fs.op_touch(self.ctx, "dataset/photos/a.jpg", "2020-01-02T00:00:00", None)
        self.assertEqual(int(os.stat(self.media / "dataset/photos/a.jpg").st_mtime), 1577923200)
        qa_fs.op_touch(self.ctx, "dataset/photos", None, 400)
        qa_fs.op_chmod(self.ctx, "000", "dataset/photos/a.jpg")
        self.assertEqual(os.stat(self.media / "dataset/photos/a.jpg").st_mode & 0o777, 0)
        with self.assertRaises(qa_fs.Refused):
            qa_fs.op_chmod(self.ctx, "4755", "dataset/photos/a.jpg")
        qa_fs.op_mv(self.ctx, "dataset/photos/a.jpg", "dataset/photos/b.jpg")
        with self.assertRaises(qa_fs.Refused):
            qa_fs.op_mv(self.ctx, "dataset/photos/b.jpg", str(self.outside))
        os.symlink(self.outside, self.media / "dataset" / "link")
        with self.assertRaises(qa_fs.Refused):
            qa_fs.op_chmod(self.ctx, "777", "dataset/link")
        qa_fs.op_rm(self.ctx, "dataset/link", False)
        self.assertEqual(self.outside.read_text(), "keep", "removing a link keeps its target")
        with self.assertRaises(qa_fs.Refused):
            qa_fs.op_rm(self.ctx, "dataset/photos", False)
        os.chmod(self.media / "dataset" / "photos", 0o000)
        qa_fs.op_rm(self.ctx, "dataset/photos", True)
        self.assertFalse((self.media / "dataset" / "photos").exists())

    def test_edge_files_are_bounded(self):
        with self.assertRaises(qa_fs.Refused):
            qa_fs.op_write_edge(self.ctx, "symlink-inside", "link.jpg")
        qa_fs.op_write_edge(self.ctx, "png", "zażółć gęślą.png")
        self.assertTrue((self.media / "edge" / "zażółć gęślą.png").read_bytes().startswith(b"\x89PNG"))
        qa_fs.op_write_edge(self.ctx, "symlink-inside", "link.jpg")
        self.assertEqual(os.readlink(self.media / "edge" / "link.jpg"), "zażółć gęślą.png")
        for bad in ("../x", "a/b", "x" * 256):
            with self.assertRaises(qa_fs.Refused):
                qa_fs.op_write_edge(self.ctx, "zero-byte", bad)
        for index in range(qa_fs.EDGE_LIMIT - 2):
            qa_fs.op_write_edge(self.ctx, "zero-byte", f"z{index}.jpg")
        with self.assertRaises(qa_fs.Refused):
            qa_fs.op_write_edge(self.ctx, "zero-byte", "one-too-many.jpg")

    def test_copy_selection_and_sandbox(self):
        dataset = self.root / "ds"
        for name in ("zdjecia_002", "zdjecia_001", "source", "for-claude-tests", "videos"):
            (dataset / name).mkdir(parents=True)
        self.assertEqual(qa_fs.copy_sources(str(dataset), ["source", "zdjecia_*:first"]), ["source", "zdjecia_001"])
        self.assertEqual(qa_fs.copy_sources(str(dataset), ["*"]), ["source", "videos", "zdjecia_001", "zdjecia_002"])
        with self.assertRaises(qa_fs.Refused):
            qa_fs.copy_sources(str(dataset), ["missing"])
        argv = qa_fs.sandbox_argv("/m/media", "/r/qa_fs.py", ["rm", "x"])
        self.assertEqual(argv[:5], ["bwrap", "--ro-bind", "/", "/", "--dev"])
        binds = [argv[i + 1] for i, a in enumerate(argv) if a == "--bind"]
        self.assertEqual(binds, ["/m/media"], "only the media copy is writable")
        self.assertIn("--unshare-net", argv)

    def test_session_context_cross_checks(self):
        repo_case = FakeRepo("setUp")
        repo_case.setUp()
        try:
            session = "d" * 32
            tmpdir = Path(os.path.realpath(repo_case.dataset)) / "for-claude-tests" / "q"
            scratch = tmpdir / "media-tagger-control-abc"
            (scratch / "media").mkdir(parents=True)
            (scratch / "owner").write_text(session)
            manifest = {"id": session, "status": "ready", "media": str(scratch / "media"), "scratch": str(scratch)}
            lib.write_json(repo_case.repo / "artifacts" / "app-control" / session / "session.json", manifest)
            run = lib.load_run(repo_case.run_dir)
            with self.assertRaises(qa_fs.Refused):
                qa_fs.session_context(repo_case.repo, run)
            run["active_session"] = {"id": session, "block": "S2"}
            ctx = qa_fs.session_context(repo_case.repo, run)
            self.assertEqual(ctx["media"], str(scratch / "media"))
            (scratch / "owner").write_text("e" * 32)
            with self.assertRaises(qa_fs.Refused):
                qa_fs.session_context(repo_case.repo, run)
            (scratch / "owner").write_text(session)
            lib.write_json(repo_case.repo / "artifacts" / "app-control" / session / "session.json",
                           {**manifest, "media": str(self.media)})
            with self.assertRaises(qa_fs.Refused):
                qa_fs.session_context(repo_case.repo, run)
        finally:
            repo_case.tearDown()


class QaSessionTests(unittest.TestCase):
    def test_sandbox_argv(self):
        existing = {"/h/.cargo", "/h/.cache", "/h/.local/share/com.example.mediatagger"}
        argv = qa_session.sandbox_argv("/h/Documents/repo", "/data/set", "/h", "/tmp/rt", "/data/set/for-claude-tests/q",
                                       False, existing=lambda p: p in existing)
        joined = " ".join(argv)
        self.assertTrue(joined.startswith("bwrap --dev-bind / / --ro-bind /h /h --ro-bind /data/set /data/set "
                                          "--bind /h/.cargo /h/.cargo --bind /h/.cache /h/.cache "
                                          "--bind /h/Documents/repo /h/Documents/repo "
                                          "--bind /data/set/for-claude-tests /data/set/for-claude-tests"), joined)
        self.assertEqual([argv[i + 1] for i, a in enumerate(argv) if a == "--tmpfs"],
                         ["/h/.local/share/com.example.mediatagger"])
        tail = argv[argv.index("--"):]
        self.assertEqual(tail[:3], ["--", "dbus-run-session", "--"])
        self.assertIn("TMPDIR=/data/set/for-claude-tests/q", tail)
        self.assertIn("XDG_RUNTIME_DIR=/tmp/rt", tail)
        self.assertNotIn("VITE_MEDIATAGGER_PERF=1", tail)
        self.assertEqual(tail[-4:], ["bun", "run", "app:control", "start"])
        perf = qa_session.sandbox_argv("/r", "/d", "/h", "/tmp/rt", "/d/for-claude-tests/q", True, existing=lambda p: False)
        self.assertIn("VITE_MEDIATAGGER_PERF=1", perf)
        with self.assertRaises(lib.AuditError):
            qa_session.sandbox_argv("/r", "/d", "/h", "/tmp/rt", "/tmp/q", False, existing=lambda p: False)

    def test_socket_length_and_ready_parsing(self):
        self.assertEqual(qa_session.socket_path_length("/x"), 2 + len(qa_session.SOCKET_SUFFIX))
        self.assertEqual(qa_session.parse_ready("BUILD x\nREADY {\"id\": \"abc\"}\n"), {"id": "abc"})
        self.assertIsNone(qa_session.parse_ready("Startup failed: x"))
        env = dict(os.environ, DISPLAY=":0", VITE_MEDIATAGGER_PERF="1", TMPDIR="/elsewhere")
        with mock.patch.dict(os.environ, env, clear=True):
            scrubbed = qa_session.session_env()
        for key in ("DISPLAY", "VITE_MEDIATAGGER_PERF", "TMPDIR"):
            self.assertNotIn(key, scrubbed)

    def test_write_descriptor_detection(self):
        root = Path(os.path.realpath(tempfile.mkdtemp(prefix="audit-fd-")))
        try:
            (root / "for-claude-tests").mkdir()
            profile = root / "profile"
            profile.mkdir()
            reader = open(root / "read.txt", "w+")
            reader.close()
            read_only = open(root / "read.txt")
            allowed = open(root / "for-claude-tests" / "w.txt", "w")
            violations = qa_session.write_fd_violations([os.getpid()], str(root), [str(profile)])
            self.assertEqual(violations, [])
            home = root / "home"
            (home / "repo").mkdir(parents=True)
            in_repo = open(home / "repo" / "ok.txt", "w")
            in_home = open(home / "notes.txt", "w")
            homed = qa_session.write_fd_violations([os.getpid()], str(root / "elsewhere"), [], home=str(home),
                                                   allowed=[str(home / "repo")])
            self.assertEqual([v["target"] for v in homed], [str(home / "notes.txt")])
            in_repo.close()
            in_home.close()
            bad = open(root / "bad.txt", "a")
            prof = open(profile / "media.db", "w")
            targets = sorted(v["target"] for v in qa_session.write_fd_violations([os.getpid()], str(root), [str(profile)]))
            self.assertEqual(targets, [str(root / "bad.txt"), str(profile / "media.db")])
            for handle in (read_only, allowed, bad, prof):
                handle.close()
            self.assertIn(os.getpid(), qa_session.descendants(os.getppid()))
            self.assertTrue(qa_session.sample(os.getpid())["rss_kb"] > 0)
        finally:
            shutil.rmtree(root, ignore_errors=True)


class QaHookTests(unittest.TestCase):
    DATASET = "/data/set"

    def decide(self, command=None, tool="Bash", scope="agent", **payload):
        body = {"tool_name": tool, "tool_input": {"command": command} if command is not None else payload.pop("input", {}),
                **payload}
        return qa_hook.evaluate(body, scope, self.DATASET)[0]

    def test_allowed_commands(self):
        script = ".claude/skills/audit/scripts/"
        for command in (
            "bun run app:control click 0123 'button[aria-label=\"Open settings\"]'",
            "bun run app:control native 0123 key Control+l",
            f"python3 {script}qa_fs.py touch dataset/a.jpg --days-ago 400",
            f"python3 {script}qa_fs.py --run docs/review/x rm dataset/a.jpg",
            f"python3 -I {script}qa_session.py mark --block S1 --suite QA-02 begin",
            f"python3 {script}audit_state.py put qa-suite <<'EOF'\n{{\"x\": \"`code` $(not run)\"}}\nEOF",
            "sqlite3 -readonly /data/set/for-claude-tests/q/media-tagger-control-1/data/x/media.db 'select count(*) from assets'",
            "tail -n 50 artifacts/app-control/abc/console.jsonl | grep -i error",
            "grep -c error artifacts/app-control/abc/process.log 2>/dev/null || true",
            "git log --oneline -5",
            "sed -n 1,40p docs/development/audit/qa-scenarios.md",
            "ls -la /data/set/for-claude-tests/q",
        ):
            self.assertTrue(self.decide(command), command)
        self.assertTrue(self.decide(tool="Read", input={"file_path": "/data/set/for-claude-tests/q/x.png"}))

    def test_blocked_commands(self):
        script = ".claude/skills/audit/scripts/"
        for command in (
            "rm -rf /data/set/for-claude-tests/q/x",
            "touch artifacts/x", "cp a b", "mv a b", "chmod 777 x", "ln -s a b", "dd if=/dev/zero of=x",
            "echo hi > notes.txt", "cat x >> y", "cat x | tee y",
            "bun run app:control start", "bun run app:control stop 0123", "bun x something",
            f"python3 {script}qa_session.py stop", f"python3 {script}qa_session.py start --block S1",
            f"python3 {script}clear_halt.py --confirm-inspected",
            f"python3 {script}bg.py start tools",
            f"python3 {script}audit_state.py assign-ids --kind qa",
            "python3 -c 'import os' # /.claude/skills/audit/scripts/qa_fs.py",
            f"python3 -c x {script}qa_fs.py",
            "python3 other.py",
            "MEDIATAGGER_QA_FS_INNER=1 python3 .claude/skills/audit/scripts/qa_fs.py rm x",
            f"python3 {script}qa_fs.py rm $(cat list)",
            "ls `pwd`",
            "cat /data/set/photos/a.jpg",
            "ls /data/set",
            "ls /data/set/for-claude-tests/../photos",
            "cat ~/.local/share/com.example.mediatagger/media.db",
            "sqlite3 /tmp/x.db 'select 1'",
            "sqlite3 -readonly x.db '.shell rm -rf /'",
            "sqlite3 -readonly x.db \"select writefile('/tmp/x', 'y')\"",
            "sqlite3 -readonly x.db 'attach database \"y\" as y'",
            "find . -delete", "find . -exec rm {} ;",
            "sed -i s/a/b/ file", "sort -o out in", "rg --pre ./evil x",
            "git checkout main", "git diff --output=x", "git -c core.pager=sh log",
            "kill -9 1234", "pkill media_tagger", "bwrap --bind / / sh",
            "awk '{print > \"x\"}' y", "xargs rm",
            f"python3 {script}audit_state.py put raw <<'EOF'\n{{}}\nEOF\nrm -rf x",
            f"python3 {script}audit_state.py put raw <<EOF\n{{}}\nEOF",
            f"cat x <<'EOF'\nhi\nEOF",
            "ls\nrm x",
        ):
            self.assertFalse(self.decide(command), command)
        for tool in ("Edit", "Write", "NotebookEdit"):
            self.assertFalse(self.decide(tool=tool, input={"file_path": "/data/set/for-claude-tests/x"}))
        self.assertFalse(self.decide(tool="Read", input={"file_path": "/data/set/photos/a.jpg"}))
        self.assertFalse(self.decide(tool="Grep", input={"pattern": "x", "path": "/data/set"}))

    def test_default_deny_and_unknown_agents(self):
        for tool in ("WebFetch", "WebSearch", "mcp__docs__create", "Task"):
            self.assertFalse(self.decide(tool=tool, input={"url": "https://example.com"}), tool)
        for tool in ("StructuredOutput", "TodoWrite"):
            self.assertTrue(self.decide(tool=tool, input={}), tool)
        body = {"tool_name": "Bash", "tool_input": {"command": "ls"}, "agent_id": "x"}
        self.assertFalse(qa_hook.evaluate(body, "settings", None, session_active=True)[0], "unknown type, session on")
        self.assertTrue(qa_hook.evaluate(body, "settings", None, session_active=False)[0], "no session: allowed")
        self.assertTrue(qa_hook.evaluate({**body, "tool_name": "Read"}, "settings", None, True)[0])
        self.assertTrue(qa_hook.evaluate({"tool_name": "Bash", "tool_input": {"command": "ls"}}, "settings", None, True)[0],
                        "main session")
        self.assertFalse(self.decide("sqlite3 -readonly x.db '.archive -x'"))

    def test_settings_scope_filters_agent_type(self):
        self.assertTrue(self.decide("rm -rf x", scope="settings"), "main session is not filtered")
        self.assertTrue(self.decide("rm -rf x", scope="settings", agent_id="a1", agent_type="review-rust"))
        self.assertFalse(self.decide("rm -rf x", scope="settings", agent_id="a1", agent_type="qa-tester"))
        self.assertFalse(self.decide("rm -rf x", scope="settings", agent_id="a1", agent_type="qa-refuter"))
        self.assertFalse(self.decide("rm -rf x", scope="agent"))

    def test_cli_exit_codes(self):
        import subprocess
        hook = str(HERE / "qa_hook.py")
        self.assertTrue(os.access(hook, os.X_OK), "hook scripts must be executable")
        blocked = subprocess.run([hook, "--scope", "agent"], input=json.dumps(
            {"tool_name": "Bash", "tool_input": {"command": "rm -rf x"}, "cwd": "/nonexistent"}),
            capture_output=True, text=True)
        self.assertEqual(blocked.returncode, 2)
        self.assertIn("qa_hook blocked", blocked.stderr)
        allowed = subprocess.run([hook, "--scope", "settings"], input=json.dumps(
            {"tool_name": "Bash", "tool_input": {"command": "rm -rf x"}, "cwd": "/nonexistent"}),
            capture_output=True, text=True)
        self.assertEqual(allowed.returncode, 0)
        garbage = subprocess.run([hook], input="not json", capture_output=True, text=True)
        self.assertEqual(garbage.returncode, 2)


class BackgroundJobTests(FakeRepo):
    def test_child_refuses_unexpected_commands(self):
        import bg
        run_rel = f"docs/review/{RUN}"
        self.assertEqual(bg.job_argv("tools", run_rel, False)[0], "sh")
        self.assertIn("--perf", bg.job_argv("sweep", run_rel, True))
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            code = bg.main(["--run", run_rel, "_child", "tools", "--", "sh", "-c", "touch pwned"])
        self.assertEqual(code, 2)
        self.assertFalse((self.repo / "pwned").exists())
        with contextlib.redirect_stderr(err):
            self.assertEqual(bg.main(["--run", run_rel, "wait", "tools"]), 2, "never started")


class SweepAndCoverageTests(unittest.TestCase):
    def test_sweep_items_cover_every_spec(self):
        flags = run_e2e_sweep.spec_flags((REPO / "docs/development/testing.md").read_text())
        self.assertEqual(flags["native-video-escape"], "MEDIATAGGER_NATIVE_VIDEO")
        self.assertEqual(flags["video-speed"], "MEDIATAGGER_NATIVE_VIDEO")
        items = run_e2e_sweep.items(REPO, perf=True)
        names = [item["item"] for item in items]
        specs = sorted(p.name.removesuffix(".e2e.js") for p in (REPO / "e2e/specs").glob("*.e2e.js"))
        for spec in specs:
            self.assertIn(spec, names)
        self.assertIn("animations+reduced-motion", names)
        self.assertEqual(next(i for i in items if i["item"] == "grid-baseline")["env"],
                         {"MEDIATAGGER_GRID_BASELINE": "1", "MEDIATAGGER_PERF": "1"})
        self.assertIn("gallery-scale-gallery-million", names)
        self.assertIn("rust-grid-baseline", names)
        command = run_e2e_sweep.command_for(next(i for i in items if i["item"] == "about"))
        self.assertEqual(command[:3], ["dbus-run-session", "--", "xvfb-run"])
        with tempfile.TemporaryDirectory(prefix="mediatagger-sweep-test-") as root:
            with mock.patch.dict(os.environ, {"DISPLAY": ":0", "MEDIATAGGER_ABOUT": "1"}):
                env = run_e2e_sweep.private_env(Path(root), {"MEDIATAGGER_GALLERY_100K": "1"})
        self.assertNotIn("DISPLAY", env)
        self.assertNotIn("MEDIATAGGER_ABOUT", env)
        self.assertEqual(env["MEDIATAGGER_GALLERY_100K"], "1")

    def test_reduced_motion_env_is_private(self):
        with tempfile.TemporaryDirectory(prefix="mediatagger-sweep-test-") as root:
            for reduced, value in ((True, "false"), (False, "true")):
                sub = Path(root) / str(reduced)
                sub.mkdir()
                extra = {"MEDIATAGGER_REDUCED_MOTION": "1"} if reduced else {}
                env = run_e2e_sweep.private_env(sub, extra, motion=True)
                self.assertEqual(env["GSETTINGS_BACKEND"], "keyfile")
                self.assertTrue(env["XDG_CONFIG_HOME"].startswith(str(sub)))
                config = sub / "config"
                self.assertIn(f"gtk-enable-animations={value}", (config / "gtk-3.0/settings.ini").read_text())
                self.assertIn("[Settings]", (config / "gtk-3.0/settings.ini").read_text())
                self.assertIn(f"enable-animations={value}", (config / "glib-2.0/settings/keyfile").read_text())
            plain = Path(root) / "plain"
            plain.mkdir()
            self.assertNotIn("GSETTINGS_BACKEND", run_e2e_sweep.private_env(plain, {}))
        inner = run_e2e_sweep.command_for({"kind": "spec", "spec": "e2e/specs/about.e2e.js"})[-1]
        self.assertIn('MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"', inner)

    def test_coverage_inventory(self):
        import coverage
        controls = coverage.inventory(REPO)
        values = {(c["kind"], c["value"]) for c in controls}
        self.assertIn(("data-testid", "gallery-timeline"), values)
        self.assertIn(("id", "settings-language-select"), values)
        self.assertTrue(any(c["scale_only"] for c in controls))
        control = {"kind": "data-testid", "value": "gallery-timeline"}
        self.assertTrue(coverage.is_touched(control, 'click ["[data-testid=\\"gallery-timeline\\"]"]'))
        self.assertFalse(coverage.is_touched(control, "click [\"#tile-size\"]"))


if __name__ == "__main__":
    unittest.main()
