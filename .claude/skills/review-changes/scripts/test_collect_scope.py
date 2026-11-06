"""Exercise scope collection against disposable Git repositories."""

from pathlib import Path
import subprocess
import tempfile
import unittest

from collect_scope import ScopeError, collect, glob_regex

RULE = """---
paths:
  - "src-tauri/src/db/**"
  - "src/library/**"
---

# Database

Read `docs/subsystems/database.md`.
"""


class ScopeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "synthetic@example.invalid")
        self.git("config", "user.name", "Synthetic fixture")
        self.write(".claude/rules/database.md", RULE)
        self.write("src-tauri/src/db/query.rs", "fn query() {}\n")
        self.write("docs/readme.md", "# Synthetic\n")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.root), *args], check=True,
                              capture_output=True, text=True).stdout

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "-qm", message)

    def paths(self, scope):
        return {f["path"]: f for f in scope["files"]}

    def test_glob_regex(self):
        self.assertTrue(glob_regex("src/library/**").match("src/library/a/b.ts"))
        self.assertTrue(glob_regex("src/**/*worker*").match("src/x/search.worker.ts"))
        self.assertTrue(glob_regex("src/**/*worker*").match("src/worker.ts"))
        self.assertTrue(glob_regex("src-tauri/tauri*.conf.json").match("src-tauri/tauri.conf.json"))
        self.assertFalse(glob_regex("src-tauri/tauri*.conf.json").match("src-tauri/tauri.conf.dev.json"))
        self.assertFalse(glob_regex("src/*.ts").match("src/a/b.ts"))

    def test_empty_on_clean_main(self):
        scope = collect(self.root)
        self.assertTrue(scope["empty"])
        self.assertEqual(scope["base"], scope["head"])
        self.assertEqual(scope["suggested_lenses"], [])

    def test_branch_includes_committed_and_uncommitted_work(self):
        base = self.git("rev-parse", "HEAD").strip()
        self.git("checkout", "-qb", "feature")
        self.write("src-tauri/src/db/query.rs", "fn query() { let _ = 1; }\n")
        self.commit("change query")
        self.write("src/library/cache.ts", "export const cache = 1;\n")  # untracked
        self.write("docs/readme.md", "# Changed\n")  # unstaged
        scope = collect(self.root)
        files = self.paths(scope)
        self.assertEqual(scope["base"], base)
        self.assertEqual(set(files), {"src-tauri/src/db/query.rs", "src/library/cache.ts", "docs/readme.md"})
        self.assertEqual(files["src/library/cache.ts"]["status"], "A?")
        self.assertEqual(files["src-tauri/src/db/query.rs"]["category"], "rust")
        self.assertTrue(files["src/library/cache.ts"]["hot_path"])
        self.assertEqual([c["subject"] for c in scope["commits"]], ["change query"])
        self.assertEqual(scope["docs"], ["docs/subsystems/database.md"])
        self.assertEqual(scope["suggested_lenses"],
                         ["review-conventions", "review-rust", "review-frontend", "review-performance"])

    def test_rename_and_delete(self):
        self.git("checkout", "-qb", "feature")
        self.git("mv", "src-tauri/src/db/query.rs", "src-tauri/src/db/queries.rs")
        self.git("rm", "-q", "docs/readme.md")
        files = self.paths(collect(self.root))
        self.assertEqual(files["src-tauri/src/db/queries.rs"]["status"], "R")
        self.assertEqual(files["src-tauri/src/db/queries.rs"]["old_path"], "src-tauri/src/db/query.rs")
        self.assertEqual(files["docs/readme.md"]["status"], "D")

    def test_small_docs_change_is_solo(self):
        self.write("docs/readme.md", "# Synthetic\n\nOne more line.\n")
        scope = collect(self.root)
        self.assertFalse(scope["empty"])
        self.assertEqual(scope["suggested_lenses"], [])

    def test_large_docs_change_skips_performance(self):
        self.write("docs/plan.md", "line\n" * 400)
        self.assertEqual(collect(self.root)["suggested_lenses"], ["review-conventions"])

    def test_range_staged_and_path_filter(self):
        first = self.git("rev-parse", "HEAD").strip()
        self.write("src/library/cache.ts", "export const cache = 1;\n")
        self.commit("add cache")
        self.write("src-tauri/src/db/query.rs", "fn query() { let _ = 2; }\n")
        self.git("add", "src-tauri/src/db/query.rs")
        self.write("docs/readme.md", "# Unstaged\n")

        ranged = collect(self.root, [f"{first}..HEAD"])
        self.assertEqual(ranged["mode"], "range")
        self.assertEqual(set(self.paths(ranged)), {"src/library/cache.ts"})

        staged = collect(self.root, staged=True)
        self.assertEqual(set(self.paths(staged)), {"src-tauri/src/db/query.rs"})

        filtered = collect(self.root, ["src-tauri"], base=first)
        self.assertEqual(set(self.paths(filtered)), {"src-tauri/src/db/query.rs"})
        self.assertIn("-- src-tauri", filtered["diff_command"])

    def test_bare_revision_is_base(self):
        first = self.git("rev-parse", "HEAD").strip()
        self.write("src/library/cache.ts", "export const cache = 1;\n")
        self.commit("add cache")
        scope = collect(self.root, ["HEAD~1"])
        self.assertEqual(scope["base"], first)
        self.assertEqual(scope["path_filter"], [])
        self.assertEqual(set(self.paths(scope)), {"src/library/cache.ts"})

    def test_rejects_bad_input(self):
        with self.assertRaises(ScopeError):
            collect(self.root, ["nope..HEAD"])
        with self.assertRaises(ScopeError):
            collect(self.root, ["/etc"])
        with self.assertRaises(ScopeError):
            collect(self.root, base="does-not-exist")


if __name__ == "__main__":
    unittest.main()
