"""Tests for the gpui-port scripts. Run: python3 -m unittest discover -s .claude/skills/gpui-port/scripts"""
from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import job  # noqa: E402
import port_state as port  # noqa: E402
import review_scope  # noqa: E402

REPO = port.repo_root()
WORKFLOW = (REPO / ".claude/workflows/gpui-port-epic.js").read_text(encoding="utf-8")


def options(**overrides) -> argparse.Namespace:
    values = {"epic": "E00", "start_from": "implement", "models": None, "max_review_rounds": 6, "max_fix_rounds": 5,
              "max_gate_rounds": 3, "allow_dirty": False, "force": False, "dry_run": False}
    return argparse.Namespace(**{**values, **overrides})


class RealManifest(unittest.TestCase):
    """The tracked manifest stays consistent with the tree."""

    def test_manifest_is_valid(self):
        self.assertEqual(port.validate(REPO, port.load_manifest(REPO)), [])

    def test_readme_table_is_rendered_from_the_manifest(self):
        text = (REPO / port.PLAN / "README.md").read_text(encoding="utf-8")
        self.assertEqual(port.rendered_readme(text, port.load_manifest(REPO)), text,
                         "run port_state.py render after editing epics.json")

    def test_every_epic_file_has_status_and_notes(self):
        for epic in port.load_manifest(REPO)["epics"]:
            text = (REPO / port.PLAN / epic["file"]).read_text(encoding="utf-8")
            self.assertRegex(text, r"## Status\n\n.+\n\n## Notatki agentów", epic["file"])

    def test_models_cover_every_role_the_workflow_uses(self):
        roles = port.load_models(REPO, None)
        workflow = (REPO / ".claude/workflows/gpui-port-epic.js").read_text(encoding="utf-8")
        for role in roles:
            self.assertIn(role, workflow, f"role {role} is not used by the workflow")
        for role in ("implementer", "implementer_hard", "test_author", "fixer", "fixer_hard", "clerk",
                     "parity_reviewer", "lens", "lens_rules", "verifier_high", "verifier_low", "scribe"):
            self.assertIn(role, roles)



def workflow_function(name: str) -> str:
    """The source of one top-level function of the workflow script."""
    start = WORKFLOW.index(f"function {name}(")
    return WORKFLOW[start:WORKFLOW.index("\n}\n", start) + 3]


def run_in_node(body: str, hard: bool = False):
    """Evaluate `body` (a JavaScript expression) next to the workflow's lens functions and return its value."""
    core = re.search(r"^const CORE_LENSES = .+;$", WORKFLOW, re.M).group(0)
    script = "\n".join([
        f"const hard = {json.dumps(hard)};", core, "const log = () => {};",
        workflow_function("scopeProblem"), workflow_function("betterScope"), workflow_function("pickLenses"),
        f"console.log(JSON.stringify({body}));",
    ])
    done = subprocess.run(["node", "-e", script], capture_output=True, text=True, check=False)
    if done.returncode != 0:
        raise AssertionError(done.stderr)
    return json.loads(done.stdout)


class ReviewLenses(unittest.TestCase):
    """Which lenses a review round runs. The scope reaches the workflow through an agent, so it can be incomplete."""

    def lenses(self, scope: dict, review_round: int = 1, hard: bool = False, last: dict | None = None) -> list[str]:
        call = f"pickLenses({json.dumps(scope)}, {review_round}, {json.dumps(last or {})}).map((lens) => lens.agent)"
        return run_in_node(call, hard)

    def problem(self, scope: dict) -> str:
        return run_in_node(f"scopeProblem({json.dumps(scope)})")

    def test_lenses_follow_the_changed_paths(self):
        scope = {"files": [{"path": "src-gpui/crates/media_tagger_client/src/lib.rs", "category": "rust"},
                           {"path": "docs/development/testing.md", "category": "docs"}]}
        self.assertEqual(self.lenses(scope), ["review-conventions", "review-rust", "review-gpui"])
        self.assertEqual(self.lenses(scope, hard=True),
                         ["review-conventions", "review-rust", "review-gpui", "review-stability"])

    def test_a_docs_only_change_gets_no_code_lens(self):
        scope = {"files": [{"path": "docs/development/testing.md", "category": "docs"}],
                 "suggested_lenses": ["review-conventions"]}
        self.assertEqual(self.lenses(scope), ["review-conventions"])

    def test_a_lost_file_list_still_runs_the_suggested_lenses(self):
        # What the scope step of epic E08 returned: no file list, the lenses of collect_scope.py intact.
        scope = {"ok": True, "empty": False, "changed_lines": 6515, "app_source_lines": 6274,
                 "suggested_lenses": ["review-conventions", "review-rust", "review-gpui", "review-performance"]}
        self.assertEqual(self.lenses(scope, hard=True),
                         ["review-conventions", "review-rust", "review-gpui", "review-performance", "review-stability"])

    def test_a_short_file_list_is_completed_by_the_suggested_lenses(self):
        scope = {"files": [{"path": "docs/plans/gpui-port/NOTES.md", "category": "docs"}],
                 "suggested_lenses": ["review-conventions", "review-rust", "review-gpui", "review-frontend"]}
        self.assertEqual(self.lenses(scope), ["review-conventions", "review-rust", "review-gpui", "review-frontend"])

    def test_without_paths_and_suggestions_the_core_lenses_run(self):
        self.assertEqual(self.lenses({"ok": True, "empty": False}), ["review-conventions", "review-rust", "review-gpui"])

    def test_later_rounds_keep_the_core_lenses(self):
        scope = {"suggested_lenses": ["review-conventions", "review-rust", "review-gpui", "review-performance"]}
        self.assertEqual(self.lenses(scope, review_round=3, hard=True),
                         ["review-conventions", "review-rust", "review-gpui"])
        self.assertEqual(self.lenses(scope, review_round=3, hard=True, last={"review-stability": 1}),
                         ["review-conventions", "review-rust", "review-gpui", "review-stability"])

    def test_an_incomplete_scope_is_named(self):
        files = [{"path": "src-gpui/Cargo.toml"}, {"path": "docs/adr/README.md"}]
        self.assertEqual(self.problem({"files": files, "file_count": 2}), "")
        self.assertEqual(self.problem({"files": files}), "")
        self.assertIn("missing", self.problem({"empty": False}))
        self.assertIn("missing", self.problem({"files": [], "file_count": 30}))
        self.assertEqual(self.problem({"files": files, "file_count": 30}), "the file list has 2 of 30 files")

    def test_a_second_relay_never_costs_what_the_first_one_had(self):
        # Epic E09, round 1: the first relay had the suggestions but no files, the second neither.
        first = {"ok": True, "empty": False, "files": [], "file_count": 39,
                 "suggested_lenses": ["review-conventions", "review-rust", "review-gpui", "review-frontend"]}
        again = {"ok": True, "empty": False, "files": [], "file_count": 39}
        call = f"betterScope({json.dumps(first)}, {json.dumps(again)})"
        self.assertEqual(run_in_node(call)["suggested_lenses"], first["suggested_lenses"])
        full = {"ok": True, "empty": False, "file_count": 2, "untracked": ["src-gpui/a.rs"],
                "files": [{"path": "src-gpui/a.rs"}, {"path": "e2e/compare/b.js"}]}
        merged = run_in_node(f"betterScope({json.dumps(first)}, {json.dumps(full)})")
        self.assertEqual([entry["path"] for entry in merged["files"]], ["src-gpui/a.rs", "e2e/compare/b.js"])
        self.assertEqual(merged["suggested_lenses"], first["suggested_lenses"])
        self.assertEqual(merged["untracked"], ["src-gpui/a.rs"])
        self.assertEqual(run_in_node(f"scopeProblem(betterScope({json.dumps(first)}, {json.dumps(full)}))"), "")
        self.assertEqual(run_in_node(f"betterScope({json.dumps(full)}, {json.dumps(again)})")["files"], full["files"])
        for unusable in ("null", json.dumps({"ok": False, "empty": False, "error": "x"}),
                         json.dumps({"ok": True, "empty": True})):
            self.assertEqual(run_in_node(f"betterScope({json.dumps(first)}, {unusable})"), first)

    def test_the_review_loop_checks_the_scope_and_reports_what_is_left(self):
        review = workflow_function("review")
        self.assertIn("scopeProblem(scope)", review)
        self.assertIn("scopePrompt(problem)", review)
        self.assertIn("betterScope(scope, again)", review)
        self.assertIn("review.gaps.push(`round ${round}: review scope incomplete", review)


class ReviewScopeRelay(unittest.TestCase):
    """review_scope.py prints the object the clerk relays, with the fields of the workflow's SCOPE schema."""

    COLLECTED = {
        "empty": False, "base": "abc", "target": "worktree", "diff_command": "git diff -M abc",
        "totals": {"files": 2, "changed_lines": 12, "app_source_lines": 9},
        "suggested_lenses": ["review-conventions", "review-rust", "review-gpui"],
        "rules": [{"rule": ".claude/rules/gpui-port.md", "files": ["src-gpui/a.rs"]}],
        "docs": ["docs/development/testing.md"], "notes": [],
        "files": [
            {"path": "src-gpui/a.rs", "status": "A?", "old_path": None, "category": "rust", "added": 9,
             "removed": 0, "binary": False, "hot_path": True, "rules": [".claude/rules/gpui-port.md"]},
            {"path": "docs/development/testing.md", "status": "M", "old_path": None, "category": "docs",
             "added": 2, "removed": 1, "binary": False, "hot_path": False, "rules": []},
        ],
    }

    def test_the_relay_keeps_every_file_and_names_the_untracked_ones(self):
        relay = review_scope.reduce(self.COLLECTED)
        self.assertEqual(relay["files"], [
            {"path": "src-gpui/a.rs", "category": "rust", "hot_path": True},
            {"path": "docs/development/testing.md", "category": "docs", "hot_path": False}])
        self.assertEqual((relay["file_count"], relay["untracked"]), (2, ["src-gpui/a.rs"]))
        self.assertEqual((relay["changed_lines"], relay["app_source_lines"]), (12, 9))
        self.assertEqual(relay["suggested_lenses"], self.COLLECTED["suggested_lenses"])
        self.assertEqual(relay["rules"], [".claude/rules/gpui-port.md"])
        self.assertEqual((relay["ok"], relay["empty"]), (True, False))

    def test_the_relay_has_exactly_the_fields_of_the_workflow_schema(self):
        schema = WORKFLOW[WORKFLOW.index("const SCOPE = obj({"):]
        schema = schema[:schema.index('}, ["ok", "empty"]')]
        schema = re.sub(r"list\(\{[^}]*\}[^)]*\)", "list", schema)  # the fields of one file entry are not top-level
        fields = set(re.findall(r"\b(\w+): (?:bool|str|strs|int|list)\b", schema))
        self.assertEqual(set(review_scope.reduce(self.COLLECTED)) | {"error"}, fields)
        self.assertLessEqual(set(review_scope.failure("boom")), fields)

    def test_the_workflow_asks_for_the_relay_and_the_real_tree_gives_one(self):
        self.assertIn("review_scope.py --base", workflow_function("scopePrompt"))
        done = subprocess.run([sys.executable, str(HERE / "review_scope.py"), "--base", "HEAD"],
                              capture_output=True, text=True, check=False, cwd=REPO)
        self.assertEqual(done.returncode, 0, done.stderr)
        relay = json.loads(done.stdout)
        self.assertEqual(relay["file_count"], len(relay["files"]))
        self.assertEqual(len(done.stdout.strip().splitlines()), 1, "one compact line, easy to copy whole")

    def test_a_failing_collection_is_reported_as_an_object(self):
        done = subprocess.run([sys.executable, str(HERE / "review_scope.py"), "--base", "no-such-revision"],
                              capture_output=True, text=True, check=False, cwd=REPO)
        self.assertEqual(done.returncode, 1)
        relay = json.loads(done.stdout)
        self.assertEqual((relay["ok"], relay["empty"]), (False, False))
        self.assertTrue(relay["error"])


class ScreenshotComparison(unittest.TestCase):
    """The parity step: a comparison that gives no screenshots is repaired or reported, never passed over."""

    FAILED = {"ok": False, "available": True, "error": "compare:ui failed for search-applied", "pairs": [],
              "failures": [{"command": "bun run compare:ui search-applied", "summary": "timed out"}]}
    PAIRS = {"ok": True, "available": True,
             "pairs": [{"scenario": "search-applied", "tauri": "t.png", "gpui": "g.png"}]}
    FIXED = {"ok": True, "summary": "fixed the scenario"}

    def run_parity(self, answers: list, checks_ok: bool = True) -> dict:
        script = "\n".join([
            'const input = { parityAvailable: true, run: "docs/review/gpui-port/EXX" };',
            'const epic = { id: "EXX", parity: ["search-suggestions", "search-applied"] };',
            'const MAX_PARITY = 2, fixerRole = "fixer", COMPARED = {}, PARITY = {}, FIXED = {};',
            "const log = () => {}, jobPrompt = () => '', parityPrompt = () => '', parityFixPrompt = () => '';",
            "const compareFixPrompt = (compared, round) => String(round);",
            f"const answers = {json.dumps(answers)}, calls = [];",
            "const ask = async (role, prompt, schema, extra) => { calls.push(`${role}: ${extra.label}`); return answers.shift() ?? null; };",
            f"const stabilize = async () => ({{ ok: {json.dumps(checks_ok)}, fixes: 0, failures: [] }});",
            "async " + workflow_function("parity"),
            "(async () => {",
            "  const state = { parity: { rounds: 0, open: [], minor: [] }, checks: {}, blocked: null };",
            "  const go_on = await parity(state);",
            "  console.log(JSON.stringify({ go_on, parity: state.parity, blocked: state.blocked, calls, left: answers.length }));",
            "})();",
        ])
        done = subprocess.run(["node", "-e", script], capture_output=True, text=True, check=False)
        if done.returncode != 0:
            raise AssertionError(done.stderr)
        return json.loads(done.stdout)

    def test_a_failed_comparison_is_repaired_and_then_assessed(self):
        # Epic E12: the first comparison timed out on a scenario step; nobody looked at the screenshots.
        result = self.run_parity([self.FAILED, self.FIXED, self.PAIRS, {"diffs": []}])
        self.assertEqual(result["calls"], ["clerk: compare #1", "fixer: fix compare #1", "clerk: compare #2",
                                           "parity_reviewer: parity #2"])
        self.assertEqual((result["parity"]["rounds"], result["parity"]["error"], result["go_on"]), (2, None, True))

    def test_a_comparison_that_never_runs_keeps_its_error(self):
        result = self.run_parity([self.FAILED, self.FIXED, self.FAILED, self.FIXED, self.FAILED])
        self.assertEqual(result["calls"], ["clerk: compare #1", "fixer: fix compare #1", "clerk: compare #2",
                                           "fixer: fix compare #2", "clerk: compare #3"])
        self.assertEqual(result["parity"]["rounds"], 0)
        self.assertEqual(result["parity"]["error"], "compare:ui failed for search-applied")
        self.assertTrue(result["go_on"])

    def test_a_missing_harness_and_a_silent_clerk_are_told_apart(self):
        missing = self.run_parity([{"ok": False, "available": False}])
        self.assertEqual(missing["calls"], ["clerk: compare #1"])
        self.assertEqual(missing["parity"]["error"], "comparison produced no screenshots")
        silent = self.run_parity([None, self.FIXED, self.PAIRS, {"diffs": []}])
        self.assertEqual(silent["calls"][1], "fixer: fix compare #1")
        self.assertIsNone(silent["parity"]["error"])

    def test_checks_that_fail_after_a_comparison_fix_block_the_epic(self):
        result = self.run_parity([self.FAILED, self.FIXED], checks_ok=False)
        self.assertEqual((result["go_on"], result["blocked"]), (False, "checks fail after comparison fixes"))

    def test_differences_are_still_fixed_and_minor_ones_end_the_loop(self):
        notable = {"diffs": [{"scenario": "search-applied", "severity": "notable", "where": "x"}]}
        minor = {"diffs": [{"scenario": "search-applied", "severity": "minor", "where": "y"}]}
        result = self.run_parity([self.PAIRS, notable, self.FIXED, self.PAIRS, minor])
        self.assertEqual(result["calls"], ["clerk: compare #1", "parity_reviewer: parity #1", "fixer: fix parity #1",
                                           "clerk: compare #2", "parity_reviewer: parity #2"])
        self.assertEqual((result["parity"]["rounds"], result["parity"]["open"], len(result["parity"]["minor"])),
                         (2, [], 1))

    def test_the_result_of_the_epic_carries_the_comparison_error(self):
        self.assertIn("error: state.parity.error ?? null", WORKFLOW)
        self.assertIn("parity.error", workflow_function("recordPrompt"))
        compared = WORKFLOW[WORKFLOW.index("const COMPARED = obj({"):]
        self.assertIn("failures", compared[:compared.index('}, ["ok", "available"]')])


class Commands(unittest.TestCase):
    def test_allowed(self):
        for command in ("cargo test --manifest-path src-tauri/Cargo.toml --locked -p media_tagger_client",
                        "cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check", "bun run rust:clippy",
                        "node --test e2e/compare", "python3 -m unittest discover -s scripts"):
            self.assertIsNone(port.command_problem(command), command)

    def test_refused(self):
        for command in ("cargo test -p media_tagger_client", "cargo install foo --locked", "bun run test:all",
                        "bun run test:e2e:tauri", "bun install", "rm -rf x", "cargo test --locked && rm x",
                        "python3 script.py", "node script.js", "sh -c 'cargo test --locked'", "bun run app:control stop",
                        "cargo test --locked $(id)", ""):
            self.assertIsNotNone(port.command_problem(command), command)

    def test_job_steps_come_from_the_manifest(self):
        steps = job.steps_for(REPO, "E06", "checks", [])
        self.assertIn(["bun", "run", "rust:fmt"], steps)
        self.assertTrue(all(step[0] in ("cargo", "bun", "node", "python3") for step in steps))
        self.assertEqual(job.steps_for(REPO, "E06", "test-all", [])[0][:2], ["dbus-run-session", "--"])

    def test_compare_needs_a_known_scenario_and_the_harness(self):
        with self.assertRaises(port.PortError):
            job.steps_for(REPO, "E08", "compare", ["no-such-scenario"])


class TempRepo(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="gpui-port-test-"))
        self.addCleanup(shutil.rmtree, self.root, True)
        shutil.copytree(REPO / port.PLAN, self.root / port.PLAN)
        (self.root / port.MODELS).parent.mkdir(parents=True)
        shutil.copy(REPO / port.MODELS, self.root / port.MODELS)
        (self.root / "package.json").write_text('{"scripts": {}}')
        (self.root / ".gitignore").write_text("/docs/review/\n")
        manifest = port.load_manifest(self.root)
        for epic in manifest["epics"]:  # the temp tree has no sources, and no epic has run in it
            epic["sources"], epic["tests_sources"], epic["docs"] = [], [], []
            if epic["status"] != "deferred":
                epic["status"] = "todo"
        port.write_json(self.root / port.PLAN / "epics.json", manifest)
        for command in (["init", "-q", "-b", "feature/x"], ["add", "-A"],
                        ["-c", "user.name=t", "-c", "user.email=t@example.invalid", "commit", "-q", "-m", "init"]):
            subprocess.run(["git", "-C", str(self.root), *command], check=True)

    def test_set_status_updates_manifest_table_epic_and_notes(self):
        port.set_status(self.root, "E00", "done", "gotowe | z kreską", "tester")
        manifest = port.load_manifest(self.root)
        self.assertEqual(port.find_epic(manifest, "E00")["status"], "done")
        readme = (self.root / port.PLAN / "README.md").read_text(encoding="utf-8")
        self.assertRegex(readme, r"\| E00 \|.*\| done \d{4}-\d\d-\d\d \|")
        epic = (self.root / port.PLAN / "E00-spike-skeleton.md").read_text(encoding="utf-8")
        self.assertRegex(epic, r"## Status\n\ndone \d{4}-\d\d-\d\d\n\n## Notatki agentów")
        last = (self.root / port.PLAN / "NOTES.md").read_text(encoding="utf-8").splitlines()[-1]
        self.assertIn("| E00 | tester | done | gotowe / z kreską |", last)
        with self.assertRaises(port.PortError):
            port.set_status(self.root, "E00", "finished")

    def test_next_follows_dependencies(self):
        manifest = port.load_manifest(self.root)
        self.assertEqual(port.next_epic(manifest)["id"], "E00")
        port.set_status(self.root, "E00", "done")
        self.assertEqual(port.next_epic(port.load_manifest(self.root))["id"], "E01")

    def test_args_refuses_unmet_dependencies_deferred_and_dirty_tree(self):
        with self.assertRaisesRegex(port.PortError, "waits on E00"):
            port.build_args(self.root, options(epic="E01"))
        with self.assertRaisesRegex(port.PortError, "deferred"):
            port.build_args(self.root, options(epic="E20"))
        (self.root / "stray.txt").write_text("x")
        with self.assertRaisesRegex(port.PortError, "not clean"):
            port.build_args(self.root, options())

    def test_args_records_the_base_and_marks_in_progress(self):
        head = port.git(self.root, "rev-parse", "HEAD")
        result = port.build_args(self.root, options(models="implementer=fable:xhigh"))
        self.assertEqual(result["base"], head)
        self.assertEqual(result["epic"]["id"], "E00")
        self.assertEqual(result["models"]["implementer"], {"model": "fable", "effort": "xhigh"})
        self.assertFalse(result["parityAvailable"])
        self.assertEqual(port.find_epic(port.load_manifest(self.root), "E00")["status"], "in progress")
        # A later step reuses the recorded base even though the tree is dirty now.
        again = port.build_args(self.root, options(start_from="review"))
        self.assertEqual(again["base"], head)

    def test_later_step_needs_an_earlier_run(self):
        with self.assertRaisesRegex(port.PortError, "start from implement"):
            port.build_args(self.root, options(start_from="gate"))

    def test_dry_run_changes_no_tracked_file(self):
        result = port.build_args(self.root, options(epic="E05", dry_run=True))
        self.assertTrue(result["dryRun"])
        self.assertEqual(port.git(self.root, "status", "--porcelain"), "")

    def test_refuses_main_and_bad_model_overrides(self):
        subprocess.run(["git", "-C", str(self.root), "checkout", "-q", "-b", "main"], check=True)
        with self.assertRaisesRegex(port.PortError, "refusing to run on main"):
            port.build_args(self.root, options())
        for bad in ("nobody=opus", "clerk=gpt", "clerk=opus:extreme", "clerk"):
            with self.assertRaises(port.PortError):
                port.load_models(self.root, bad)

    def run_checks(self, command):
        manifest = port.load_manifest(self.root)
        port.find_epic(manifest, "E00")["checks"] = [command]
        port.write_json(self.root / port.PLAN / "epics.json", manifest)
        started = job.start(self.root, "E00", "checks", [])
        self.assertEqual(started["status"], "running")
        return job.wait(self.root, "E00", "checks", 60)

    def test_checks_job_runs_detached_and_reports_each_step(self):
        passed = self.run_checks("python3 -m unittest -h")
        self.assertEqual((passed["ok"], passed["status"], passed["code"]), (True, "exited", 0))
        self.assertEqual([step["code"] for step in passed["steps"]], [0])
        failed = self.run_checks("python3 -m unittest no_such_module_gpui_port")
        self.assertEqual((failed["ok"], failed["code"]), (False, 1))
        self.assertEqual(failed["failed"], ["python3 -m unittest no_such_module_gpui_port"])
        self.assertIn("no_such_module_gpui_port", failed["tail"])
        self.assertEqual(port.git(self.root, "status", "--porcelain", "--", "docs/review"), "")

    def test_display_jobs_respect_the_app_control_lock(self):
        lock = self.root / job.APP_CONTROL_LOCK
        lock.parent.mkdir(parents=True)
        lock.write_text("{}")
        self.assertEqual(job.start(self.root, "E00", "test-all", [])["status"], "busy")
        self.assertTrue(lock.exists(), "a foreign lock is never removed")


if __name__ == "__main__":
    unittest.main()
