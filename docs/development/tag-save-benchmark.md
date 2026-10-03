# Tag-save benchmark

Measured 2026-10-03 with [tag_save_benchmark.rs](../../src-tauri/tests/tag_save_benchmark.rs).
The baseline is `d634c2efad4801747833307dde3f5dc32cb67ae1`; the second run uses the
uncommitted tag-save changes over that revision. Both runs used the same benchmark
source, SHA-256 `affba36e0e7d4cb37458b2b3639995a567bdbc831d885db2c55ece3e8e7f0b36`.
The archived baseline had only that Rust test added. Both builds used the same
generated frontend assets because Tauri requires `frontendDist` even when compiling
backend tests. Neither benchmark starts the frontend.

The baseline and current measurements ran sequentially, in that order, after the
full verification suite finished. No other repository tests or builds were launched
during timing. These are single diagnostic runs without fixed CPU frequency,
release optimization or timing assertions; small differences are not precise ratios.

## Environment and method

| Property | Value |
| --- | --- |
| Host | x86-64 desktop, 8 cores / 16 threads |
| System | Arch-based Linux, 7.x kernel |
| Rust | rustc 1.98.0, 88d9e12ae, 2026-08-18 |
| Cargo profile | Default `test`, unoptimized with debug information, identical target directory |
| SQLite binding | rusqlite 0.32.1, bundled libsqlite3-sys 0.30.1, identical Cargo.lock |
| Database | A fresh file-backed WAL database for each scenario, `synchronous=NORMAL` |
| Storage | Temporary databases under `/tmp`, a tmpfs on this host |
| Samples | Twelve calls per operation, fixture construction excluded |

Each row crosses 1,000/10,000/50,000 vocabulary names with 1/15/30 initial tags per
target asset and 1/4/16 background assignments per name. The mean is the elapsed
microseconds divided by twelve, truncated to an integer. Single replacements
alternate the original list and an equally sized replacement. Bulk calls target
ten assets. Bulk additions use an existing vocabulary name on each call and grow
the target tag sets. Background assets retain every vocabulary name throughout.

The [CSV](tag-save-benchmark.csv) includes all 27 cases per version for each date.
Filter `measured_on=2026-10-03` for this run. The table below selects 30 initial tags
per target and sixteen background assignments per vocabulary name.

| Vocabulary | Operation | Before, ms | After, ms |
| --- | --- | ---: | ---: |
| 1,000 | Single no-op | 8.122 | 0.565 |
| 1,000 | Single replacement | 8.768 | 1.295 |
| 1,000 | Bulk no-op, ten assets | 68.135 | 5.815 |
| 1,000 | Bulk existing-name addition, ten assets | 87.834 | 14.603 |
| 10,000 | Single no-op | 76.450 | 0.566 |
| 10,000 | Single replacement | 77.409 | 1.334 |
| 10,000 | Bulk no-op, ten assets | 629.106 | 5.916 |
| 10,000 | Bulk existing-name addition, ten assets | 751.455 | 15.131 |
| 50,000 | Single no-op | 387.295 | 0.566 |
| 50,000 | Single replacement | 388.703 | 1.314 |
| 50,000 | Bulk no-op, ten assets | 3167.515 | 5.824 |
| 50,000 | Bulk existing-name addition, ten assets | 3745.505 | 15.235 |

The optimized measurements no longer show the baseline's growth with vocabulary
and background-assignment size. Cost still increases with the target tag count.
This benchmark measures backend mutation helpers. It does not measure desktop
interaction latency, worker transfer time, disk-backed production storage, or
long-session behavior.

## Reproduction

Run from the checkout containing the current changes. Build the frontend first so
both Tauri test builds can embed the same assets. Keep the Cargo profile and host
conditions the same and do not run the two versions concurrently.

```sh
repo=$PWD
benchmark_root=$(mktemp -d /tmp/mediatagger-tag-benchmark-XXXXXX)
mkdir "$benchmark_root/baseline"
git archive d634c2e | tar -x -C "$benchmark_root/baseline"
cp src-tauri/tests/tag_save_benchmark.rs "$benchmark_root/baseline/src-tauri/tests/"
bun run build
ln -s "$repo/dist" "$benchmark_root/baseline/dist"
export CARGO_TARGET_DIR="$repo/src-tauri/target"
cargo test --manifest-path "$benchmark_root/baseline/src-tauri/Cargo.toml" \
  --locked --test tag_save_benchmark -- --ignored --nocapture \
  > "$benchmark_root/before.log" 2>&1
cargo test --manifest-path "$repo/src-tauri/Cargo.toml" \
  --locked --test tag_save_benchmark -- --ignored --nocapture \
  > "$benchmark_root/after.log" 2>&1
```

Both logs must report one passed diagnostic test and contain 27 numeric CSV rows.
Keep raw logs locally. No worktree, schema migration, or benchmark-only production
code is needed.

## Verification and historical samples

The separate [2026-10-03 verification](testing.md#tag-save-verification-on-2026-10-03)
passed the full `bun run test:all` command with tag-editor E2E enabled. The real
Tauri scenario verifies zero vocabulary-page requests for unchanged names and
one page for changed names in its small initialized dictionary, plus persistence
after frontend reload. These checks establish behavior rather than latency.

Rows dated 2026-09-17 in the CSV retain the earlier measurements. That run used the
same baseline and unoptimized test profile, but verification processes overlapped
parts of the measurements and its complete environment metadata was not recorded.
Those historical rows are not evidence of the new full-suite verification.
