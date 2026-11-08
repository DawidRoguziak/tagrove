# Sources for the `known` verdict

A judge may return `known` only when one of these files already describes the issue (same
mechanism, not just the same area). `known_source` names the file and the section, for example
`docs/plans/grid-performance/10-open-topics.md#...` or `docs/adr/0008-...md Consequences`.

| Source | What counts |
| --- | --- |
| `docs/plans/grid-performance/07-verification-and-docs.md`, section "Otwarte tematy" | open topics after the grid-performance verification |
| `docs/plans/grid-performance/10-open-topics.md` | the open-topics list |
| `docs/plans/grid-performance/NOTES.md` | recorded limitations and agent notes |
| `docs/adr/0001-*.md` … `docs/adr/0010-*.md`, section "Consequences" | accepted trade-offs of each decision |
| `docs/development/testing.md`, sections "Known limitations" and "Troubleshooting" | harness and environment limits |
| owning `docs/` pages | behavior documented as intended (verdict `rejected`, not `known`) |

Known at planning time (examples, not an exhaustive list):

- asset protocol scope `"**"`;
- no HiDPI thumbnails;
- the 16,000,000 px windowed threshold tied to WebKitGTK 2.54;
- a 1.6 s outlier in `gallery.performance`;
- timeline months computed in UTC;
- a 1M selection write holds the write lock for 2–3 s;
- a large range does not survive a result change (`stale`);
- the global `start_asset_query` token;
- unverified backup compatibility from zip 2 to zip 9;
- advisories in Tauri/GTK dependencies;
- "element click intercepted" retries in ui.redesign, settings-maintenance, group-order and
  review-fixes.

Not known (they become `origin: recon` candidates and go through a refuter and a judge): the
entries of [recon-candidates.json](recon-candidates.json).
