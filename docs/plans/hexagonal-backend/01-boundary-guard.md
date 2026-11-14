# Etap 01: strażnik granicy, snapshot wire, ADR 0034 (Proposed)

Fala A. Warstwa: testy Rust i dokumentacja. Kod produkcyjny bez zmian.

Dokumentacja do przeczytania:
- [README planu](README.md) (reguły 1–4, decyzje D1–D4);
- `docs/architecture/system-overview.md` (granice modułów);
- `docs/architecture/ipc-contract.md` (checklista zmiany kontraktu);
- `docs/adr/README.md` (kiedy i jak pisać ADR).

## Cel

Zamrozić obecny stan i dać etapom 02–08 mierzalne wyjście:
- **strażnik granicy** mówi, które pliki rdzenia jeszcze używają Tauri/GTK/libmpv, a jego allowlista może tylko maleć;
- **snapshot wire** łapie przypadkową zmianę kontraktu IPC podczas przenoszenia kodu.

## Problemy (potwierdzone w kodzie, commit `397181f`)

- **Tauri/GTK/libmpv są w 14 plikach, a nie w 13.** README pomijało `services/video_events.rs` (`use libmpv2::Mpv; use libmpv2_sys as sys;`).
  - Stały zbiór adaptera (7): `lib.rs` i 6 plików `commands/*`.
  - Przejściowa allowlista reguły 1 (7):
    - `app/state.rs` (03),
    - `services/progress.rs` (02),
    - `services/thumb_protocol.rs` (05),
    - `services/thumb_service.rs` (02, tylko test),
    - `services/video_events.rs` (04),
    - `services/video_player_service.rs` (04),
    - `video_surface.rs` (04).
- **Reguła 3 jest dziś łamana** przez `video_surface.rs:16` (`crate::commands::video::{VideoBounds, VideoControlLabels}`), więc potrzebna jest osobna allowlista z jednym wpisem do etapu 04.
- **Reguła 2 dziś przechodzi.** `db` sięga tylko do `models`, `error`, `utils`, `thumbs` i siebie.
- `async-channel` i `libloading` są zależnościami tylko dla Linuksa i występują wyłącznie w `video_surface.rs`, więc trafiły na listę zakazanych identyfikatorów.

## Kroki

1. Napisać `src-tauri/tests/architecture_boundaries.rs`, jeden cel testowy z dwiema częściami:
   - **Skaner** (tylko std): usuwa komentarze i literały z zachowaniem linii, tokenizuje i rozwija `crate::{…}` oraz `super::super::`. Ma samotesty.
   - **`import_rules`**: reguły 1–3, test „allowlista tylko maleje” i test istnienia wpisów. Reguła 4 jest testem `#[ignore]` do etapu 06.
   - **Snapshot wire:**
     - posortowana lista 45 komend z `generate_handler!`, każda opisana w `ipc-contract.md`;
     - jedna celowo niezarejestrowana komenda (`rename_asset_file`);
     - sygnatury wire wszystkich komend: argumenty bez wstrzykiwanych `State`/`AppHandle`/`WebviewWindow`, ścieżki skrócone do ostatniego segmentu, plus typ zwracany;
     - moduł `dto_snapshot` z JSON-em DTO odpowiedzi, wejścia i kanału wideo (każdy wariant enumów tagowanych, odrzucanie nieznanych tagów);
     - nazwy emitowanych eventów (`process-progress`).
2. Napisać ADR 0034 w stanie Proposed i dodać go do indeksu.
3. Zaktualizować dokumentację, reguły i skille (lista w [Pliki](#pliki)).
4. Uruchomić próby mutacyjne i pełny `test:backend`.

## Pliki

- Nowe:
  - `src-tauri/tests/architecture_boundaries.rs`
  - `docs/adr/0034-tauri-independent-backend-core.md`
  - ten plik
- Zmienione:
  - `docs/adr/README.md`
  - `docs/architecture/system-overview.md`: akapit o granicy pod tabelą modułów, mapa weryfikacji
  - `docs/architecture/ipc-contract.md`: „Relevant tests”, punkt 11 checklisty
  - `docs/development/testing.md`
  - `CLAUDE.md`: lista celów
  - `.claude/rules/ipc-contract.md`
  - `.claude/skills/tauri-testing/skills/tauri-rust-tests/SKILL.md`
  - `.claude/skills/audit/references/batches.json`: batch B15b
  - `README.md` i `NOTES.md` planu
- Bez zmian: `src-tauri/src/**`, `Cargo.toml`, `Cargo.lock`, frontend.

## Testy

- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- Próby mutacyjne (każda cofnięta) z oczekiwanym wynikiem:
  - `use tauri::AppHandle;` w `services/csv_service.rs` → reguła 1;
  - `use crate::services::db_pool;` w `db/queries.rs` → reguła 2;
  - `use crate::commands::scan;` w `indexer.rs` → reguła 3;
  - komentarz `// tauri gtk` i literał `"tauri://x"` w `indexer.rs` → brak trafienia;
  - zmiana nazwy parametru `session_id` → `session` w `get_asset_query_page` → diff sygnatury `-`/`+`;
  - `#[serde(rename = "x")]` na `ScanSummary` → `dto_snapshot`;
  - usunięcie `services/progress.rs` z allowlisty → reguła 1;
  - dopisanie czystego pliku do allowlisty → „remove it from FRAMEWORK_ALLOWLIST”.
- `bun run rust:fmt`, `bun run rust:clippy`, `bun run test:backend`.

## Kryteria akceptacji

- Nowy cel jest zielony. Allowlista reguły 1 ma 7 wpisów, allowlista reguły 3 ma 1 wpis.
- Każda próba mutacyjna daje czerwony test z komunikatem wskazującym plik i linię.
- `git diff -- src-tauri/src` jest pusty.
- ADR 0034 jest w indeksie ze statusem Proposed.

## Ryzyka

- Skaner nie widzi kodu z makr. Etap 08 zastępuje regułę 1 kompilatorem i `cargo tree`.
- Snapshot sygnatur zależy od nazw typów (ostatni segment ścieżki). Przeniesienie typu bez zmiany nazwy go nie rusza. Zmiana nazwy typu wymaga świadomej aktualizacji tabeli, a test wypisuje gotową.

## Status

done (2026-11-14). Commit czeka na akceptację użytkownika.

## Notatki agentów

### 2026-11-14 — agent etapu 01
- Zrobione: cel `architecture_boundaries` (23 testy + 1 `#[ignore]`), ADR 0034 Proposed, dokumentacja, reguła IPC, skill `tauri-rust-tests`, batch audytu.
- Odkryte:
  - 14 plików zamiast 13 (`services/video_events.rs:10-11`);
  - reguła 3 łamana przez `video_surface.rs:16`;
  - `rename_asset_file` celowo niezarejestrowane (`.claude/rules/ipc-contract.md`), zamrożone w `UNREGISTERED_COMMANDS`;
  - wszystkie wejściowe DTO są publiczne, więc snapshot pokrywa je w teście integracyjnym bez zmian widoczności:
    - `AssetMetaFilterInput`, `BulkMediaGroupUpdateInput`,
    - `SelectionSpec`, `SelectionMutation`,
    - `VideoBounds`, `VideoControlLabels`, `VideoControl`.
- Stan wyjściowy reguły 4 (`-- --ignored`): `commands/scan.rs` 24, `assets.rs` 21, `thumbs.rs` 6, `import_export.rs` 3 trafienia.
- Zostało: commit po akceptacji.
- Testy:
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries` → pass (23 passed, 1 ignored);
  - próby mutacyjne → wszystkie zgodnie z oczekiwaniem;
  - `bun run rust:fmt` → pass;
  - `bun run rust:clippy` → pass;
  - `python3 -I -m unittest` (skrypty audytu) → pass;
  - `bun run test:backend` → pass (lib 359, `architecture_boundaries` 23, pozostałe cele zielone).
