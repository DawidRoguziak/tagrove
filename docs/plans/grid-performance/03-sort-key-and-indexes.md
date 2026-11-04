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

todo — 2026-11-04

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->
