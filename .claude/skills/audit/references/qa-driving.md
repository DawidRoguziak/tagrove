# Driving QA sessions

You drive a running session; the clerk started it. Use the literal session ID from your brief
wherever this page writes `<id>`, and `docs/review/<run>` for `<run>`. The full command
reference is in [verify-mediatagger](../../verify-mediatagger/SKILL.md) (Drive, Native input,
Session states) and `bun run app:control help`.

## Loop

1. `bun run app:control doctor <id>`: continue only with `state: ok`. It prints the scan root
   (`<scratch>/media`) and the app-data directory.
2. Act with one command at a time: `click`, `dblclick`, `modclick`, `fill`, `select`, `keys`,
   `drag`, `wheel`, `scroll`, `resize`, `native`, `alert`.
3. Observe: `wait <selector|idle> [state] [ms]`, `inspect [selector]`, `screenshot` (then Read
   the PNG), `logs`.
4. Check the oracle the catalog names before you call a step passed.

## Oracles

- **UI state:** `inspect` with a narrow selector; after a write, close and reopen the view or
  use `refresh` (it records a console gap).
- **Database:** `sqlite3 -readonly <app-data>/media.db '<select>'`; app data is
  `<scratch>/data/com.example.mediatagger.e2e/`. Read-only queries only; never open the DB
  while a backup import runs.
- **Files in the copy:** `ls -la`, `stat` below `<scratch>/media/`. Change them only with
  `qa_fs.py`.
- **Logs:** `artifacts/app-control/<id>/console.jsonl` (frontend console, window errors,
  unhandled rejections, alerts, capture gaps) and `process.log` (driver, WebKit and app
  stdout/stderr, Rust panics and perf lines). `actions.jsonl` has every command with its
  timestamp; cite those lines as evidence.

## Traps

- **Native file picker:** press `native key Control+l`, `native type` exactly the media root
  that doctor prints (`<scratch>/media`), then `native key Return`. Never choose another folder.
  Cancel with `native key Escape`. A save path in a folder that does not exist makes GTK show
  its own "The folder contents could not be displayed" dialog; doctor reports `unresponsive`
  (observe, `native key Escape`, `stop`). Screenshot it, dismiss it with `native key Escape`,
  then cancel the picker with another `native key Escape`.
- **Backups:** restore only a backup exported in the same session, from inside the copy.
- **Close window:** the custom Close button ends the session. Use it only as the last step of
  the last suite of a block, and only if the suite says so.
- **Tile loader:** `[data-testid="tile-thumb-loader"]` is always in the DOM and becomes
  visible after 300 ms. Wait with `wait '<selector>' hidden`, never `gone`.
- **Backup export/import, Clear library, Remove root:** the maintenance gate and the library
  locks wait without a limit, so doctor reports `busy-maintenance` and every action is refused.
  Before a long backup step run `qa_session.py --run <run> maintenance --minutes 60`, then wait
  with `wait`, `logs` and `process.log` until doctor says `ok`. Record the duration.
- **Alerts:** deleting a file whose source is missing or whose cleanup is pending opens a
  `window.alert`; doctor reports `alert-open`. Read it with `alert <id> text`, then
  `alert <id> accept`.
- **Timeline scrubber:** it shows only when results span more than one month. If the copy has
  one month, spread modification times first (`qa_fs.py touch dataset/<folder> --days-ago N`
  on a few subfolders), then Rescan all.
- **Windowed mode and the position bar** need more than 16,000,000 px of grid height; a 30k
  library never reaches it. Report it as a gap covered by the `gallery-100k` sweep and the
  `gallery-scale` harness.
- **Xvfb limits (not bugs):** no window manager (maximize, minimize and minimum size cannot be
  proven), MP4 picture areas may be black, `:focus-visible` needs a native click.
- **Modifier gestures:** use `modclick` (it releases keys); never leave a modifier pressed with
  `keys`.
- **Fast scroll:** after a jump of half a viewport or more, `[data-scroll-paused]` is set for
  150 ms; loaded tiles stay visible and new ones ask only the cache (`c=1`).
- **Language:** sessions start in English. Switch with `select '#settings-language-select'
  value=<code>` and switch back to `en` at the end of the scenario.

## Evidence

Every bug cites at least one screenshot or `inspect` result, the `actions.jsonl` lines of the
steps, and any console/process entries around them (with timestamps). Reproduce once before
reporting and record the frequency.
