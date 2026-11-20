"""Tests for the gpui-port scripts. Run: python3 -m unittest discover -s .claude/skills/gpui-port/scripts"""
from __future__ import annotations

import argparse
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

REPO = port.repo_root()


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
