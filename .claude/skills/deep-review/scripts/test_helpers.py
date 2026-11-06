"""Exercise deep-review record invariants against disposable Git repositories and real files."""

from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from init_review import create_review, snapshot, write_json
from validate_review import validate


class ReviewTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "synthetic@example.invalid")
        self.git("config", "user.name", "Synthetic fixture")
        (self.root / ".gitignore").write_text("/raports/\n")
        (self.root / "example.py").write_text("value = 1\n")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.root), *args], check=True,
                              capture_output=True, text=True).stdout

    def read(self, name):
        return json.loads((self.folder / name).read_text())

    def save(self, name, obj):
        write_json(self.folder / name, obj)

    def fixture(self):
        self.folder = create_review(self.root, "example", ["example.py"])
        run = self.read("run.json")
        run.update(status="complete", limitations=[])
        original = {"id": "a-001", "lens": "review-rust", "path": "example.py", "line": 1,
                    "title": "Synthetic claim", "priority": "P2", "confidence": 80, "origin": "pre-existing",
                    "trigger": "read", "impact": "wrong value", "evidence": ["example.py:1 `value = 1`"],
                    "counterevidence": "none", "fix": "Correct value"}
        for aid, role, lens in (("a", "review", "review-rust"), ("b", "review", "review-conventions"),
                                ("v", "verification", "review-verifier")):
            directory = self.folder / "agents" / aid
            directory.mkdir()
            (directory / "notes.md").write_text("Synthetic notes\n")
            write_json(directory / "findings.json", [original] if aid == "a" else [])
            run["assignments"].append({"id": aid, "role": role, "agent_type": lens, "lens": lens, "agent_id": aid,
                                       "areas": ["example"], "status": "complete", "model": "opus",
                                       "effort": "high", "notes": f"agents/{aid}/notes.md",
                                       "findings": f"agents/{aid}/findings.json", "limitations": []})
        run["coverage"] = [{"id": "example", "paths": ["example.py"], "assignments": ["a", "b"]}]
        self.save("run.json", run)
        self.save("findings.json", {"candidates": [{**original, "id": "DR-001", "original_ids": ["a-001"],
                                                  "originators": ["a"], "verdict": "confirmed", "stale": False,
                                                  "verification": "verification/DR-001.json"}], "duplicates": []})
        self.save("verification/DR-001.json", {"candidate_id": "DR-001", "agent_id": "v", "verdict": "confirmed",
                                               "basis": "code", "evidence": ["example.py:1 traced"],
                                               "counterevidence": [], "missing_fact": None, "commands": [],
                                               "verified_hashes": run["baseline"]["hashes"]})
        return run

    def test_valid_and_nested_ignored(self):
        self.fixture()
        nested = self.folder / "agents/a/nested/evidence.txt"
        nested.parent.mkdir()
        nested.write_text("synthetic\n")
        self.assertEqual(validate(self.folder, self.root), [])
        self.git("add", "-f", str(nested.relative_to(self.root)))
        self.assertIn("Generated artifacts are not all Git-ignored", validate(self.folder, self.root))

    def test_collaboration_is_optional_but_must_be_answered(self):
        run = self.fixture()
        self.assertEqual(run["handoffs"], [])
        self.assertEqual(validate(self.folder, self.root), [])
        run["handoffs"] = [{"id": "h1", "sender": "a", "recipient": "b", "question": "Guard?",
                            "evidence": "agents/a/notes.md", "response": None, "status": "pending"}]
        self.save("run.json", run)
        self.assertTrue(any("Unanswered handoff" in x for x in validate(self.folder, self.root)))

    def test_effort_levels(self):
        run = self.fixture()
        run["assignments"][2]["effort"] = "max"
        self.save("run.json", run)
        self.assertEqual(validate(self.folder, self.root), [])
        run["assignments"][0]["effort"] = "low"
        self.save("run.json", run)
        self.assertTrue(any("Effort must be one of" in x for x in validate(self.folder, self.root)))

    def test_coverage_requires_distinct_lenses(self):
        run = self.fixture()
        run["assignments"][1]["lens"] = "review-rust"
        self.save("run.json", run)
        self.assertTrue(any("two independent lenses" in x for x in validate(self.folder, self.root)))

    def test_collisions_preserve_existing(self):
        now = datetime(2026, 1, 1, tzinfo=timezone.utc)
        first = create_review(self.root, "same", now=now)
        (first / "report.md").write_text("preserved")
        second = create_review(self.root, "same", now=now)
        self.assertNotEqual(first, second)
        self.assertEqual(second.name, first.name + "-2")
        self.assertEqual((first / "report.md").read_text(), "preserved")

    def test_default_changes_including_deleted_and_untracked(self):
        (self.root / "example.py").unlink()
        (self.root / "added.py").write_text("new = True\n")
        self.git("add", "example.py")
        self.folder = create_review(self.root)
        run = self.read("run.json")
        self.assertEqual(run["schema_version"], 2)
        self.assertEqual(set(run["scope"]["included"]), {"example.py", "added.py"})
        self.assertIsNone(run["baseline"]["hashes"]["example.py"])
        self.assertFalse(run["scope"]["no_changes"])

    def test_branch_default_includes_committed_range(self):
        base = self.git("rev-parse", "HEAD").strip()
        self.git("checkout", "-qb", "feature")
        (self.root / "feature.py").write_text("feature = 1\n")
        self.git("add", "feature.py")
        self.git("commit", "-qm", "feature work")
        self.folder = create_review(self.root)
        run = self.read("run.json")
        self.assertEqual(run["baseline"]["comparison_base"], base)
        self.assertEqual(run["baseline"]["committed"], ["feature.py"])
        self.assertEqual(run["scope"]["included"], ["feature.py"])
        self.assertFalse(run["scope"]["no_changes"])

    def test_range_mode_ignores_working_tree(self):
        base = self.git("rev-parse", "HEAD").strip()
        (self.root / "ranged.py").write_text("ranged = 1\n")
        self.git("add", "ranged.py")
        self.git("commit", "-qm", "ranged")
        (self.root / "dirty.py").write_text("dirty = 1\n")
        self.folder = create_review(self.root, "ranged", commit_range=f"{base}..HEAD")
        run = self.read("run.json")
        self.assertEqual(run["scope"]["included"], ["ranged.py"])
        self.assertEqual(run["baseline"]["untracked"], [])
        with self.assertRaisesRegex(ValueError, "do not combine"):
            create_review(self.root, "bad", ["example.py"], base="HEAD")

    def empty_review(self):
        self.folder = create_review(self.root)
        run = self.read("run.json")
        run.update(status="complete", limitations=[])
        self.save("run.json", run)
        return run

    def test_empty_diff(self):
        run = self.empty_review()
        self.assertTrue(run["scope"]["no_changes"])
        self.assertEqual(validate(self.folder, self.root), [])

    def test_no_changes_rejects_committed_range_and_cli_fails(self):
        base = self.git("rev-parse", "HEAD").strip()
        (self.root / "example.py").write_text("value = 2\n")
        self.git("commit", "-am", "changed source")
        run = self.empty_review()
        run["baseline"]["comparison_base"] = base
        self.save("run.json", run)
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertIn("Invalid no-changes claim: recorded commit range has changes",
                      validate(self.folder, self.root))
        result = subprocess.run(
            [sys.executable, "-B", str(Path(__file__).with_name("validate_review.py").resolve()),
             str(self.folder)], cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn("recorded commit range has changes", result.stdout)

    def test_no_changes_accepts_different_commits_with_equal_trees(self):
        base = self.git("rev-parse", "HEAD").strip()
        (self.root / "example.py").write_text("value = 2\n")
        self.git("commit", "-am", "changed source")
        (self.root / "example.py").write_text("value = 1\n")
        self.git("commit", "-am", "restore source")
        run = self.empty_review()
        self.assertNotEqual(base, run["baseline"]["head"])
        run["baseline"]["comparison_base"] = base
        self.save("run.json", run)
        self.assertEqual(validate(self.folder, self.root), [])

    def test_no_changes_uses_recorded_head_after_checkout(self):
        self.empty_review()
        (self.root / "example.py").write_text("value = 2\n")
        self.git("commit", "-am", "later source")
        self.git("checkout", "-qb", "later-branch")
        self.assertEqual(validate(self.folder, self.root), [])

    def test_no_changes_requires_available_commits(self):
        run = self.empty_review()
        for field in ("comparison_base", "head"):
            recorded = run["baseline"][field]
            for value in (None, "", "0" * 40, "--help", "HEAD:example.py"):
                with self.subTest(field=field, value=value):
                    run["baseline"][field] = value
                    self.save("run.json", run)
                    self.assertIn(f"Cannot verify no-changes claim: invalid or unavailable {field}",
                                  validate(self.folder, self.root))
            run["baseline"][field] = recorded

    def test_unavailable_base_can_be_recorded_as_partial_without_no_changes(self):
        run = self.empty_review()
        run.update(status="partial", limitations=["Comparison base unavailable"])
        run["baseline"]["comparison_base"] = None
        run["scope"]["no_changes"] = False
        self.save("run.json", run)
        self.assertEqual(validate(self.folder, self.root), [])

    def test_incomplete_assignment_and_partial(self):
        run = self.fixture()
        run["assignments"][1].update(status="blocked", limitations=["Tool unavailable"])
        self.save("run.json", run)
        self.assertTrue(any("Incomplete assignment" in x for x in validate(self.folder, self.root)))
        run.update(status="partial", limitations=["Second lens blocked"])
        self.save("run.json", run)
        self.assertEqual(validate(self.folder, self.root), [])

    def test_self_confirmation_and_missing_reference(self):
        self.fixture()
        verdict = self.read("verification/DR-001.json")
        verdict.update(agent_id="a", evidence_files=["missing.txt"])
        self.save("verification/DR-001.json", verdict)
        errors = validate(self.folder, self.root)
        self.assertTrue(any("Invalid independent verifier" in x for x in errors))
        self.assertTrue(any("Missing or unsafe" in x for x in errors))

    def test_duplicates_keep_all_originators(self):
        self.fixture()
        original = self.read("agents/a/findings.json")[0]
        original.update(id="b-001", lens="review-conventions")
        self.save("agents/b/findings.json", [original])
        findings = self.read("findings.json")
        findings["candidates"][0].update(original_ids=["a-001", "b-001"], originators=["a", "b"])
        findings["duplicates"] = [{"original_id": x, "canonical_id": "DR-001"} for x in ("a-001", "b-001")]
        self.save("findings.json", findings)
        self.assertEqual(validate(self.folder, self.root), [])
        findings["candidates"][0]["originators"] = ["a"]
        findings["duplicates"].pop()
        self.save("findings.json", findings)
        errors = validate(self.folder, self.root)
        self.assertTrue(any("Originator mismatch" in x for x in errors))
        self.assertTrue(any("Duplicate mappings" in x for x in errors))

    def test_stale_source_requires_recheck_or_uncertain(self):
        run = self.fixture()
        (self.root / "example.py").write_text("value = 2\n")
        self.assertTrue(any("Source changes" in x for x in validate(self.folder, self.root)))
        run["source_changes"] = ["example.py"]
        self.save("run.json", run)
        self.assertTrue(any("Stale finding" in x for x in validate(self.folder, self.root)))
        findings = self.read("findings.json")
        findings["candidates"][0].update(stale=True, verdict="uncertain")
        self.save("findings.json", findings)
        verdict = self.read("verification/DR-001.json")
        verdict.update(verdict="uncertain", missing_fact="Concurrent edit requires recheck")
        self.save("verification/DR-001.json", verdict)
        self.assertEqual(validate(self.folder, self.root), [])
        verdict.update(verdict="confirmed", missing_fact=None, verified_hashes=snapshot(self.root, ["example.py"]))
        self.save("verification/DR-001.json", verdict)
        findings["candidates"][0].update(stale=False, verdict="confirmed")
        self.save("findings.json", findings)
        self.assertEqual(validate(self.folder, self.root), [])

    def test_coverage_and_lost_original(self):
        run = self.fixture()
        run["coverage"][0]["assignments"] = ["a"]
        self.save("run.json", run)
        self.save("findings.json", {"candidates": [], "duplicates": []})
        errors = validate(self.folder, self.root)
        self.assertTrue(any("two independent lenses" in x for x in errors))
        self.assertTrue(any("Original findings lost" in x for x in errors))

    def test_old_schema_rejected(self):
        run = self.fixture()
        run["schema_version"] = 1
        self.save("run.json", run)
        self.assertTrue(any("Unsupported schema version" in x for x in validate(self.folder, self.root)))

    def test_ignore_required_and_symlink_parent_rejected(self):
        (self.root / ".gitignore").write_text("")
        with self.assertRaisesRegex(ValueError, "Add /raports/"):
            create_review(self.root)
        (self.root / ".gitignore").write_text("/raports/\n")
        (self.root / "raports").symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlink"):
            create_review(self.root)


if __name__ == "__main__":
    unittest.main()
