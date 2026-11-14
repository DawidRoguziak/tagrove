# review-changes evals

These are development aids for tuning the skill and the `review-*` agents, and nothing loads
them during a review. The patches were generated against `base_commit` in `expected.json`
and apply to later commits as long as the touched files have not changed. Check with
`git apply --check` for every patch before a run; regenerate a patch that no longer applies
(see [Adding a case](#adding-a-case)) and move `base_commit` with it. Five patches
contain one seeded defect apiece. Patch `06` is a correct change used to measure false
positives.

| Patch | Seeded defect | Should be caught by |
| --- | --- | --- |
| `01-ipc-unregistered` | new command missing from `generate_handler!` and docs | conventions, rust |
| `02-lock-order` | `thumb_lock` taken before `scan_lock` in `delete_asset_service` | rust |
| `03-locale-missing` | key added to 10 of 11 locales | conventions, frontend |
| `04-stale-details` | lightbox drops its request-ID check after `await` | frontend |
| `05-selection-scan` | `isSelected(index, id)` (O(log r)) replaced by a per-tile scan of the rendered range | performance |
| `06-clean-control` | none | must report 0 confirmed findings |

## Run one case

The clone sees only committed files, so commit the skill and agents first. Use a throwaway
clone and never a worktree; `CLAUDE.md` forbids worktrees. `git clone --local`
hardlinks fail when the temp dir is on another filesystem, so use `--no-hardlinks`.

```sh
case=02-lock-order
tmp=$(mktemp -d)
git clone -q --no-hardlinks "$(git rev-parse --show-toplevel)" "$tmp/repo"
cd "$tmp/repo"
git checkout -q -b "eval/$case"   # a commit that contains this skill and .claude/agents
git apply -3 "$OLDPWD/.claude/skills/review-changes/evals/patches/$case.patch"
claude -p "/review-changes --base HEAD" > "$tmp/$case.out"
cd - && rm -rf "$tmp/repo"   # keep $tmp/$case.out for scoring
```

The patch stays uncommitted, and `--base HEAD` limits the scope to exactly the patch. Without
it, the merge base with `origin/main` could pull in unrelated commits.

## Score

For each case, take the confirmed findings from the output and compare them with
`expected.json`.

- **Hit.** A confirmed finding at the expected path, describing the same mechanism, at or
  above `min_priority`.
- **Miss.** No such finding. Read the agent's notes and check whether the lens ran
  (triage) or ran and missed the defect (heuristics).
- **False positive.** Any other confirmed finding. On `06`, every finding counts as one.

Track hits/5 and false positives/6 for each change to the skill or agents. A change that
raises hits but adds false positives on `06` is a regression. Results vary between runs, so
run each case at least twice before you conclude anything.

## Adding a case

Seed defects that represent real bug classes in this repository, taken from fixed bugs in
`git log` or rules in `.claude/rules/`. Keep each patch small and plausible, so that the
defect looks like an honest mistake, and note which lens should catch it. Generate the patch
with `git diff` in a clone, never by hand-editing the patch.
