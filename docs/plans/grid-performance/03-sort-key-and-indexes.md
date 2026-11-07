# Etap 03: kolumna sort-key, indeksy i porządki w zapytaniach

Fala B, przed etapem 04, bo oba edytują `src-tauri/src/lib.rs`. Warstwa: Rust (SQL).

Dokumentacja do przeczytania:
- `docs/subsystems/database.md` (schemat, `init_schema`, pola pochodne, porządek, rewizje);
- `docs/subsystems/library-query-and-gallery.md` (semantyka porządku);
- `docs/architecture/ipc-contract.md` (usunięcie komendy).

## Problemy (potwierdzone w kodzie)

1. **Rebuild wektora ID nie korzysta z indeksu.** `src-tauri/src/db/queries.rs:846-886`: CTE `filtered_assets` jest łączone z `grouped_bucket_stats` (agregat grup), a `ORDER BY` używa `CASE`, `trim()` i konkatenacji `'group:' || key`. Każdy rebuild to pełny skan i sortowanie w pamięci tymczasowej. Szacunek: ~100–400 ms przy 100k, sekundy przy 1M. Rebuild dzieje się przy każdej zmianie rewizji.
2. **Zduplikowane indeksy** (`src-tauri/src/db/schema.rs:98-104, 155-166`):
   - `idx_tags_name` dubluje UNIQUE na `tags.name`;
   - `idx_asset_tags_asset` dubluje wiodącą kolumnę PK;
   - `idx_asset_tags_tag` dubluje wiodącą kolumnę `idx_asset_tags_tag_asset`;
   - `idx_assets_kind` dubluje wiodącą kolumnę `idx_assets_gallery_kind_favorite`;
   - `idx_assets_modified_at` nie obsługuje obecnego porządku.

   Każdy insert skanu i każda edycja tagów aktualizuje je wszystkie.
3. **Filtr tag-exclude** (`queries.rs:820-825`) robi dla każdego assetu join do `tags` i porównuje nazwy.
4. **`list_asset_summaries_by_ids`** przygotowuje SQL od nowa przy każdym wywołaniu.
5. **Martwa komenda `list_assets`** (`COUNT(*)` + `LIMIT/OFFSET`, `queries.rs` około 327 i 418) jest zarejestrowana w `src-tauri/src/lib.rs:149` i ma wrapper TS, ale nikt jej nie woła.

## Kroki

1. **Pomiar bazowy** (razem z etapem 07, krok 0). Test diagnostyczny `#[ignore]` budujący syntetyczną bazę 100k i 1M w `tempfile`, z grupami, tagami i pustymi datami. Mierzy czas `try_list_ordered_asset_ids_with_meta` i zapisuje `EXPLAIN QUERY PLAN`. Wynik trafia do notatek.
2. **Kolumny sort-key w `assets`:**
   - `sort_bucket_time` (= `bucket_modified_at` dla grupy albo `modified_at`);
   - `sort_bucket_id` (= `bucket_max_id` albo `id`);
   - `sort_group_null_order` (0/1);
   - pozostałe klucze porządku już istnieją: `media_group_order`, `modified_at`, `id`.

   Indeks złożony na `(sort_bucket_time DESC, sort_bucket_id DESC, sort_group_null_order, media_group_order, modified_at DESC, id DESC)`. Rozważ też warianty z filtrem `kind` i `favorite`, jeśli `EXPLAIN` pokaże potrzebę.
3. **Utrzymanie kolumn.** Każde miejsce, które zmienia `modified_at`, `media_group_key` lub `media_group_order` (insert i update skanu, mutacje grup, restore, import CSV), przelicza sort-key dla **całej grupy** w tej samej transakcji. Znajdź wszystkie zapisy przez `rg "media_group_key|modified_at" src-tauri/src/db`. Rozważ triggery SQLite, jeśli są prostsze i testowalne.
4. **`init_schema`:** idempotentny krok `performance_schema_version` (wzór na istniejące wersjonowane kroki w `schema.rs`): `ALTER TABLE ADD COLUMN` (jeśli brak), backfill w jednej transakcji, `CREATE INDEX IF NOT EXISTS`, `DROP INDEX IF EXISTS` dla duplikatów. Backfill musi działać na bazie 1M w rozsądnym czasie; zmierz go.
5. **Nowe zapytanie porządku:** `ORDER BY` po kolumnach sort-key, bez CTE `grouped_bucket_stats`. **Porządek musi być identyczny bit w bit** jak obecny.
6. **Tag-exclude po ID:** najpierw rozwiąż nazwy wykluczonych tagów na ID, potem `NOT EXISTS (SELECT 1 FROM asset_tags WHERE asset_id = a.id AND tag_id IN (...))`.
7. **`prepare_cached`** w `list_asset_summaries_by_ids` (dzielony na stałe rozmiary chunków, np. 256, aby cache SQL był trafiany).
8. **Usunięcie `list_assets`** w jednej zmianie IPC:
   - funkcja w `queries.rs`, komenda w `commands/assets.rs`, wpis w `generate_handler!`;
   - wrapper w `src/platform/tauri`, typ w `contracts/`;
   - `docs/architecture/ipc-contract.md`.
9. **`PRAGMA optimize` po skanie** (`scan_service.rs` około 314): sprawdź wersję SQLite w rusqlite i czy obowiązuje `analysis_limit`; jeśli nie, ustaw `PRAGMA analysis_limit=400`.

## Pliki

- `src-tauri/src/db/schema.rs`, `db/queries.rs`, `db/mutations.rs` (utrzymanie sort-key), ewentualnie `db/scan_membership.rs`
- `src-tauri/src/services/scan_service.rs` (insert i update), usługi restore i CSV, jeśli piszą te pola
- `src-tauri/src/commands/assets.rs`, `src-tauri/src/lib.rs`
- `src/platform/tauri/*` (wrapper `list_assets`), `src/platform/tauri/contracts/*`
- `docs/subsystems/database.md`, `docs/architecture/ipc-contract.md`

## Testy

- `backend_integration`: **golden test porządku**. Fixture z grupami (z `media_group_order` i bez), pustymi kluczami grup, kluczami ze spacjami (`trim`), równymi `modified_at` i filtrami tagów. Kolejność ID przed i po zmianie musi być identyczna. Najpierw dodaj test na starej implementacji, potem zmieniaj.
- Test: sort-key aktualizuje się po:
  - zmianie grupy (dodanie, usunięcie, zmiana kolejności);
  - zmianie `modified_at` przy rescanie;
  - restore i imporcie CSV.
- Test idempotencji `init_schema`: dwukrotne uruchomienie, stara baza bez kolumn i baza po backfillu.
- Diagnostyka `#[ignore]`: czas rebuildu przy 100k i 1M przed i po (do notatek).
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test backend_integration`, `bun run rust:clippy`.

## Kryteria akceptacji

- Rebuild wektora przy 1M jest co najmniej kilkukrotnie szybszy (cel < 150 ms bez filtrów). `EXPLAIN` pokazuje użycie indeksu bez `USE TEMP B-TREE FOR ORDER BY`.
- Porządek identyczny jak przed zmianą (golden test).
- Brak `list_assets` w kodzie i dokumentacji.

## Ryzyka

- Pominięcie któregoś miejsca zapisu grupy daje błędny porządek. Mitygacja: golden test i testy per ścieżka zapisu, ewentualnie triggery.
- Backfill na dużej bazie przy starcie wydłuża pierwszy start po aktualizacji. Zmierz i, jeśli trzeba, pokaż postęp lub zrób backfill partiami.

## Status

done — 2026-11-07 (zaakceptowane i zacommitowane). Usunięcie `list_assets` (krok 8) odłożone decyzją użytkownika.

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->

### 2026-11-06 — orkiestrator (etap 07, krok 0)
- Z etapu 07: krok 1 (pomiar bazowy) zrobiony w `src-tauri/tests/grid_baseline.rs` (`query_rebuild_and_page_baseline`, release). Pełna ścieżka `AssetQueryManager::start` z rebuildem, p50:
  - bez filtrów: 30 ms (100k), 377–383 ms (1M, trzy biegi); szacunek z punktu 1 był zawyżony dla 100k;
  - wykluczenie tagu `common` (80%): 56 ms / 657–665 ms, najwolniejszy wariant (punkt 3);
  - tag `common` (20%): 18 / 224 ms; favorites: 6,5 / 88 ms; kind video: 4,7 / 61 ms; rzadki tag: 1,4 / 12 ms;
  - sesja z cache: 0,08 ms; strona 128 elementów: 0,09 / 0,14 ms.
- `EXPLAIN QUERY PLAN` obecnego zapytania bez filtrów (skopiowany SQL, sqlite3 CLI 3.54; aplikacja ma wbudowane SQLite z libsqlite3-sys 0.30.1, czyli 3.46.0): `MATERIALIZE grouped_bucket_stats` (`SCAN a`, `USE TEMP B-TREE FOR GROUP BY`), `SCAN a`, `BLOOM FILTER ON bs`, `SEARCH bs USING AUTOMATIC COVERING INDEX (sort_bucket=?)`, `USE TEMP B-TREE FOR ORDER BY`.
- Cel z kryteriów (< 150 ms przy 1M bez filtrów) to około 2,5× szybciej niż baza.

### 2026-11-07 — agent etapu 03
- Zrobione ([ADR 0004](../../adr/0004-trigger-maintained-gallery-sort-keys.md)):
  - kolumny `sort_bucket_time`, `sort_bucket_id`, `sort_group_null_order` (`performance_schema_version` 4) i indeks `idx_assets_sort_key`; backfill w jednej transakcji;
  - utrzymanie przez 6 triggerów `assets_sort_key_*`, nie przez kod. Powód: surowy SQL w fixture'ach (w tym `grid_baseline::seed_library`), staging restore i starsza binarka na tej samej bazie. Triggery odtwarzane w każdym `init_schema`, a ich nazwy usuwane przed pierwszym zapisem;
  - `DeferredSortKeys` dla zapisów bulk (`set_assets_media_group_bulk`, import CSV): znacznik `sort_keys_deferred` w transakcji, jedno przeliczenie na kubełek. Bez tego bulk 5000 do jednej grupy trwał 4,5–23 s;
  - zapytanie: bez filtrów skan indeksu pokrywającego; z filtrami statystyki kubełków po pasujących wierszach liczone w Rust (decyzja użytkownika: porządek dokładny, także z filtrami);
  - tagi po ID; wykluczenie jako nieskorelowane `NOT IN` (−80 ms przy 1M względem `NOT EXISTS` po ID);
  - `prepare_cached` w `list_asset_summaries_by_ids` (chunki 16/64/256 dopełniane NULL) i w upsercie skanu;
  - 5 zbędnych indeksów usuwanych przy starcie;
  - backup: wersje 0–4, spójność sort-key dla v4, odrzucanie obcych triggerów i widoków.
- Odkryte:
  - grupowanie liczy się po przefiltrowanych wierszach (database.md, test `ordered_asset_ids_filter_before_computing_group_stats`), więc sort-key całej grupy nie wystarcza dla zapytań z filtrami;
  - `conn.execute` przy zapisie `assets` kompiluje triggery przy każdym prepare: skan 20k trwał przez to 2,4–2,7 s, a po `prepare_cached` 0,48–0,77 s (`src-tauri/src/db/scan_membership.rs`);
  - `set_assets_media_group_bulk` czyta klucz jako String i odrzuca klucze BLOB (stan sprzed etapu; BLOB powstaje tylko przez surowy SQL);
  - `PRAGMA optimize` w SQLite 3.46 sam stosuje tymczasowy `analysis_limit` 2000 (sqlite3.c, `SQLITE_DEFAULT_OPTIMIZE_LIMIT`), więc kroku 9 nie trzeba było zmieniać;
  - `list_assets` woła 17 speców E2E, nie 11: dodatkowo animations, video-speed, lightbox-activity, review-fixes, app.smoke, popular-tags.
- Pomiary (`grid_baseline.rs`, release, maszyna wolna: bez cargo/rustc/vitest/wdio drugiej sesji; w drzewie były niezacommitowane zmiany TS etapu 04, które nie wpływają na testy Rust), przed → po, p50 przy 1M:
  - bez filtrów 384 → 32 ms (plan: `SCAN assets USING COVERING INDEX idx_assets_sort_key`, bez temp B-tree);
  - exclude_common 671 → 420 ms; tag_common 229 → 102 ms; favorites 90 → 49 ms; kind_video 62 → 36 ms; tag_rare 12 → 6 ms;
  - 100k: bez filtrów 31 → 3,8 ms, exclude_common 57 → 29 ms;
  - skan 20k: 637–873 → 480–765 ms; favorite pod skanem p50 4,3 → 2,0 ms, p95 16 → 13 ms; rebuild pod skanem 97 → 8 ms;
  - backfill przy upgrade (`init_schema`, 1M): ~1,5 s; bulk 5000 do jednej grupy: 146–171 ms.
- Desktop E2E (debug, Xvfb, XDG pod `/tmp/mediatagger-stage03-*`, `TMPDIR=/var/tmp`, profil usuwany po biegu; w drzewie niezacommitowane zmiany TS etapu 04, `bun run typecheck` przechodził):
  - `app.smoke` 4/4, `group-order` 1/1, `app.workflows` 13/13 (w tym CSV i restore);
  - `grid-baseline` (`MEDIATAGGER_GRID_BASELINE=1 MEDIATAGGER_PERF=1`), dwa biegi, 3/3: skan 20k p50 695 ms (wcześniej ~1266 ms w debug), like i tag 0/20 nieudanych, 1 rebuild podczas skanów;
  - pojedyncze zapisy 120–180 ms wypadają w fazie `remove_scan_root` (usuwanie 20k wierszy w jednej transakcji, bez ustępowania; czas usuwania bez zmian: p50 161 ms wobec 166 ms). W biegu etapu 05 też były (131 i 76 ms); teraz trafia ich tam więcej, bo skany są krótsze.
- Zostało:
  - usunięcie `list_assets` (komenda, wrapper, typ, testy Rust, 17 speców E2E): propozycja w notatce w etapie 07;
  - `remove_scan_root` nie ustępuje zapisom użytkownika (ADR 0002, znane ograniczenie).
- Testy:
  - golden test na starej implementacji przed zmianami (`backend_integration golden_order`) → pass;
  - po zmianach `cargo test --manifest-path src-tauri/Cargo.toml --locked` → pass (lib 215, backend_integration 11 i pozostałe cele);
  - `bun run rust:fmt`, `bun run rust:clippy` → pass;
  - pomiary: `TMPDIR=/var/tmp cargo test --manifest-path src-tauri/Cargo.toml --locked --release --test grid_baseline -- --ignored --nocapture --test-threads=1`.
