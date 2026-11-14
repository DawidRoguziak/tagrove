# Etap 06: warstwa use-case, cienkie komendy

Fala C. Warstwa: Rust (nowy moduł rdzenia `usecases/`, cztery pliki komend, jedna deklaracja `mod` w `lib.rs`, reguła 4 testu granicy) i dokumentacja. Kontrakt wire bez zmian.

Dokumentacja do przeczytania:
- [README planu](README.md) (sekcja 06 z notatkami „Z etapu 01” i „Z etapu 03”, „Fale i równoległość”, „Zasady dla agentów”, „Egzekucja granicy”);
- [ADR 0034](../../adr/0034-tauri-independent-backend-core.md), [ADR 0022](../../adr/0022-lightbox-delete-fails-fast-during-a-scan.md);
- `docs/architecture/system-overview.md` (locking policy, tabela modułów, opis granicy);
- `docs/architecture/ipc-contract.md` (komendy assets/scan/thumbs/import_export, teksty błędów, `get_asset_query_id_range` jako `ipc::Response`);
- `docs/subsystems/library-query-and-gallery.md` (`start_asset_query`: walidacja, kolejność żądań);
- `docs/subsystems/settings-operations.md` i `data-safety-and-portability.md` (`find_duplicate_assets`, `clear_library_data`).

## Cel

Zdjąć `#[ignore]` z reguły 4 (`commands_do_not_touch_db_or_locks` w `src-tauri/tests/architecture_boundaries.rs`) i mieć ją zieloną. Logika workflowów (walidacja, blokady, SQL, postęp) przechodzi dosłownie z `commands/{assets,scan,thumbs,import_export}.rs` do nowego modułu rdzenia `src-tauri/src/usecases/`. Handler komendy robi tylko: argumenty → `spawn_blocking` → use case → istniejące `map_err` / `tauri::ipc::Response`. Nazwy komend, klucze argumentów, pola odpowiedzi, eventy, teksty błędów, kolejność blokad i kolejność żądań się nie zmieniają.

## Problemy (potwierdzone w kodzie, commit `262be60`)

Stan wyjściowy reguły 4 (`-- --ignored`): `scan.rs` 24, `assets.rs` 21, `thumbs.rs` 6, `import_export.rs` 3 trafienia.

- `commands/assets.rs` (860 linii):
  - `:32-45` `AssetMetaFilterInput`, `:441-446` `BulkMediaGroupUpdateInput`: typy wejściowe wire, potrzebne walidacji; `dto_snapshot` importuje je z `commands::assets` (`architecture_boundaries.rs:993`).
  - `:47-82` `start_asset_query`: `normalize_query_filters` (`:194-228`) → walidacja `owner_id` → `begin_request` **przed** `spawn_blocking` (kolejność żądań w obrębie właściciela), potem `start_at` w `spawn_blocking`.
  - `:84-154` page/position/id_range/sections: `database.queries.*(&database.admit()?, …)` w `spawn_blocking`; `id_range` (`:122-139`) owija bajty w `tauri::ipc::Response`, `None` → pusta treść.
  - `:156-192` `validate_summary_batch`, `get_asset_summaries_by_ids`, `get_asset_details`: `db::` w ciele komendy.
  - `:230-535` `set_asset_tags`, `merge_asset_tags_bulk`, `set_asset_favorite`, `set_asset_media_group`, `set_assets_media_group_bulk`, `list_tags`: prywatne `*_service(&AppState)` + `log_user_write_perf` (`:356-367`).
  - `:322-354` `toggle_assets_favorite_bulk` (`db::` w ciele), `apply_selection_mutation` (serwis w ciele).
  - `:548-567` `LIBRARY_BUSY_ERROR`, `delete_asset_service` (`try_with_scan_and_thumb_lock`, ADR 0022).
  - `:569-611` `find_duplicate_assets`: SQL i dwa `emit_progress(&app, …)` inline w `spawn_blocking`, bez blokad.
  - `:628-672` `rename_asset_file_service`, `apply_duplicate_resolution_batch_service` (`with_scan_and_thumb_lock`).
  - `:674-860` testy (7) i `#[cfg(test)] normalize_new_file_name`.
- `commands/scan.rs` (375 linii): `*_service` (`:24-176`) z `with_scan_lock`/`with_scan_and_thumb_lock` i `db::`; `:178-189` `scan_startup_roots` z closure `with_scan_lock` + `scan_service::scan_roots` podawanym do `adapter::startup_scan::run_startup_scan`; testy (4) `:191-375`.
- `commands/thumbs.rs` (152 linie): `get_video_tool_status` (`thumbs::video_tool_status` inline), `render_all_thumbnails`/`render_failed_thumbnails` (`with_thumb_operation`), `clear_all_thumbnails` (`with_thumb_lock`) inline; testy anulowania (2) używają `db::` (`:81-152`). `cancel_render_all_thumbnails` (`:62-66`) woła serwis wprost, bez blokad i SQL.
- `commands/import_export.rs` (183 linie): `*_service` z `with_scan_and_thumb_lock` (`clear_library_data_service`, już na `&impl ProgressSink` od etapu 02) i `with_database_maintenance` (bundle); bez testów.
- **Luka reguły 4:** warunek łapie `db::`, tokeny `with_*lock` i `Connection`. Nie łapie `try_with_scan_and_thumb_lock` (`assets.rs:557`), `with_thumb_operation` (`thumbs.rs:38,53`) ani `with_database_maintenance` (`import_export.rs:135,161`), choć to helpery z `app/locks.rs`.
- **`lib.rs`:** nowy moduł najwyższego poziomu wymaga `pub mod usecases;`. Prompt etapu mówi, że 06 nie dotyka `lib.rs`; użytkownik zgodził się na tę jedną linię (etap 07 nie biegnie równolegle).
- Końce linii: wszystkie edytowane pliki mają LF (0 linii CRLF). `thumb_service.rs` (423 linie CRLF), `thumb_scheduler.rs` (459), `thumbs.rs`, `indexer.rs` i `main.rs` mają końce mieszane; `lib.rs` jest LF, ale jego deklaracje `mod` prowadzą do plików CRLF, więc `rustfmt` nie jest na nim uruchamiany.

## Decyzje

1. **Nazwy plików 1:1 z komendami:** `usecases/{assets,scan,thumbs,import_export}.rs` + `usecases/mod.rs`. Komenda `commands/X.rs::foo` ma use case `usecases/X.rs::foo_service`, więc ścieżki reguł `.claude/rules`, batchy audytu i dokumentacji parują się wprost.
   - Odrzucone: `library.rs` ze szkicu README. Koliduje z `src/library/` frontendu i z „library revision”, a plik zawiera też CSV i bundle. Szkic „Stan docelowy” w README poprawiony.
2. **Nazwy funkcji:** istniejące `*_service` bez zmian, więc przeniesienie widać w `git diff --color-moved`, a testy nie zmieniają treści. Ciała wydzielone z komend dostają tę samą konwencję `<komenda>_service`. Zmiana nazw to osobna kosmetyka.
3. **Zależności use case'u to to, co komenda dziś już podaje:**
   - `&AppState` dla ciał, które dziś biorą `app.state::<AppState>()` w `spawn_blocking`; obejmuje wszystkie ciała z blokadami, bo helpery z `app/locks.rs` i tak wymagają `&AppState`;
   - `&Arc<DatabaseRuntime>` dla komend zapytań i odczytów, które dziś klonują `state.database` do `spawn_blocking` (`start/page/position/id_range/sections`, `summaries_by_ids`, `details`, `toggle_assets_favorite_bulk`). `Arc`, bo `DatabaseRuntime::admit` bierze `self: &Arc<Self>` (tak samo przyjmuje go `selection_service::apply_selection_mutation`).
   - Odrzucone: wszędzie `&AppState`. `State<'_, AppState>` nie jest `'static`, więc komendy zapytań musiałyby przejść na `AppHandle`, czyli zmienić kształt adaptera, a nie tylko przenieść kod.
   - Odrzucone: wąskie struktury zależności albo traity portów per use case. Więcej kodu bez drugiej implementacji („Czego nie robimy” w README).
4. **Postęp:** use case przyjmuje `progress: &impl ProgressSink` (jak `scan_folder_service` i `clear_library_data_service`) i woła `services::progress::emit_progress`; komenda podaje `&app` (`impl` z `adapter/events.rs`). Dotyczy też `find_duplicate_assets_service`, `render_*_thumbnails_service`, `clear_all_thumbnails_service` i `scan_startup_roots_service`.
5. **Typy wejściowe wire** `AssetMetaFilterInput` i `BulkMediaGroupUpdateInput` idą do `usecases/assets.rs`, bo potrzebują ich `normalize_query_filters` i `set_assets_media_group_bulk_service`; zostawione w `commands` złamałyby regułę 3. `commands/assets.rs` robi `pub use crate::usecases::assets::{AssetMetaFilterInput, BulkMediaGroupUpdateInput};`, więc import w `dto_snapshot` i teksty `COMMAND_SIGNATURES` zostają.
6. **`start_asset_query` w dwóch krokach, kolejność bez zmian:**
   - `begin_asset_query_service(&Arc<DatabaseRuntime>, …, &owner_id, generation) -> Result<(AssetQueryFilters, u64), String>`: normalizacja, walidacja `owner_id`, `begin_request`. Wołany przed `spawn_blocking`, tak jak dziś; bez SQL i blokad workflowów (tylko licznik generacji właściciela).
   - `start_asset_query_service(&Arc<DatabaseRuntime>, filters, page_size, request_id, generation, anchor_offset) -> AppResult<…>` w `spawn_blocking`.
7. **`get_asset_query_id_range`:** use case zwraca `AppResult<Vec<u8>>` z `unwrap_or_default()` w środku (pusta treść dla nieaktualnej sesji to semantyka rdzenia); adapter owija bajty w `tauri::ipc::Response`.
8. **`scan_startup_roots`:** closure komendy woła `usecases::scan::scan_startup_roots_service(&roots, &state, &app)` (`with_scan_lock` + `scan_service::scan_roots` + `map_err`) i dalej jest podawany do `adapter::startup_scan::run_startup_scan` (notatka „Z etapu 03”).
9. **Zostaje w adapterze:** `get_startup_popular_tags` (odczyt `State`), `cancel_render_all_thumbnails` (serwis wprost, bez blokad i SQL), `apply_selection_mutation` (serwis wprost: `selection_service` sam bierze pozwolenie i połączenie), cały `commands/video.rs` (D1) i `window.rs`. Zasada: komenda nie bierze pozwolenia, połączenia ani blokady; wołanie serwisu rdzenia, który robi to sam, jest dozwolone (pośrednik bez treści nic nie wnosi). Komunikaty `"… worker failed: {e}"` (błąd `JoinError`) zostają w handlerach.
10. **Reguła 4 zaostrzona o token `locks`** (decyzja użytkownika): każdy helper z `app/locks.rs` wymaga ścieżki przez `locks`, więc warunek łapie też `try_with_…`, `with_thumb_operation` i `with_database_maintenance`. `admit()` nie jest flagowane, bo `open_video` (D1) czyta DB w adapterze.
11. **Testy idą z kodem** do `usecases/*` (reguła 4 skanuje też `#[cfg(test)]`, a testy używają `db::`). Nazwy funkcji testowych bez zmian, zmienia się tylko prefiks `commands::X::tests` → `usecases::X::tests`.
12. **Cykle:** `usecases` importuje `app`, `services`, `db`, `models`, `utils`, `thumbs`, `error`; w rdzeniu nikt nie importuje `usecases` (tylko `commands/*`), więc nie powstaje nowy cykl. Reguła 2 zabrania `db → usecases`, reguła 3 `usecases → commands/adapter/video`, reguła 1 Tauri w `usecases`. Żaden use case nie potrzebuje niczego z adaptera.
13. **Bez równoległych agentów.** README dopuszcza podział per plik komend; przeniesienie jest mechaniczne i łatwiej je zweryfikować jednym diffem.

## Kroki

1. Ten plik; status 06 w README → `in progress`.
2. Pomiar bazowy: lista `--lib -- --list`, wyniki `--lib` i `architecture_boundaries`, reguła 4 `-- --ignored`, liczby CRLF.
3. `usecases/mod.rs` i `pub mod usecases;` w `lib.rs`.
4. Przeniesienie: `assets`, `scan`, `thumbs`, `import_export`.
5. Reguła 4: bez `#[ignore]`, z tokenem `locks`, nowy doc comment.
6. Testy, `rustfmt` tylko na edytowanych plikach, `bun run rust:fmt`, `rust:clippy`, `test:backend`.
7. Próby mutacyjne.
8. Dokumentacja (lista w [Pliki](#pliki)).
9. Desktop E2E, jeden spec naraz.
10. `/review-changes` na diffie etapu.

## Pliki

- Nowe:
  - `src-tauri/src/usecases/{mod,assets,scan,thumbs,import_export}.rs`
  - ten plik
- Zmienione:
  - `src-tauri/src/lib.rs` (jedna linia `pub mod usecases;`)
  - `src-tauri/src/commands/{assets,scan,thumbs,import_export}.rs`
  - `src-tauri/tests/architecture_boundaries.rs` (tylko reguła 4)
  - `docs/architecture/system-overview.md`, `docs/architecture/ipc-contract.md`
  - `docs/subsystems/{scanning-and-indexing,data-safety-and-portability,database,thumbnails,lightbox,library-query-and-gallery}.md`
  - `docs/development/testing.md` (opis reguły 4 w `architecture_boundaries`)
  - `CLAUDE.md` („Layout”)
  - `.claude/rules/{backend-core,ipc-contract,data-safety,scanning,thumbnails}.md`
  - `.claude/skills/audit/references/batches.json`
  - `.claude/skills/tauri-testing/skills/tauri-rust-tests/SKILL.md`
  - `README.md` i `NOTES.md` planu
- Bez zmian:
  - `generate_handler!`, `COMMAND_SIGNATURES`, `REGISTERED_COMMANDS`, `dto_snapshot`, nazwy eventów, allowlisty;
  - `Cargo.toml`, `Cargo.lock`, frontend;
  - `commands/video.rs`, `commands/window.rs`, `app/locks.rs`, `services/*`, `db/*`;
  - ADR 0034 (Proposed; Context opisuje stan na `397181f`).

## Testy

- Stan wyjściowy (`262be60`): `--lib` 364 passed, 2 ignored (366 na liście `-- --list`); `architecture_boundaries` 24 passed, 1 ignored; reguła 4 `-- --ignored`: 54 trafienia (24/21/6/3).
- Przeniesione z kodem (13, nazwy bez zmian, prefiks `commands::X::tests` → `usecases::X::tests`):
  - `usecases::assets::tests`: `selection_summary_batch_limit_and_group_response_contract`, `delete_fails_fast_while_a_scan_holds_the_scan_lock_and_changes_nothing`, `delete_recovers_a_scan_lock_poisoned_by_a_panicked_holder`, cztery `normalize_new_file_name_*`;
  - `usecases::scan::tests`: `scan_commands_hold_one_connection_while_scanning`, `add_scan_root_rejects_invalid_folder_and_does_not_persist_root`, `add_scan_root_persists_valid_folder`, `remove_scan_root_deletes_assets_and_thumbnail_files`;
  - `usecases::thumbs::tests`: dwa `cancel_render_all_thumbnails_*`.
- Brak testu samego handlera `#[tauri::command]` (tak jak przed etapem; most sprawdza desktop E2E).
- Komendy:
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib` (i `-- --list`);
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`;
  - `bun run rust:fmt`, `bun run rust:clippy`, `bun run test:backend`, `git diff --check`;
  - `python3 -I -m unittest` w `.claude/skills/audit/scripts`.
- Desktop E2E (jeden spec naraz): `app.smoke`, `app.workflows`, `gallery-selection` (`MEDIATAGGER_GALLERY_SELECTION=1`), `tag-editor` (`MEDIATAGGER_TAG_EDITOR=1`, profil `XDG_DATA_HOME` pod `/tmp/mediatagger-*`), `settings-maintenance` (`MEDIATAGGER_SETTINGS_MAINTENANCE=1`).

## Kryteria akceptacji

- `architecture_boundaries`: wszystkie testy zielone, 0 ignored; tabele snapshotu bez zmian.
- `--lib`: liczba passed bez spadku, zbiór nazw testów (ostatni segment) identyczny.
- `rg 'db::|locks|with_\w*lock|Connection' src-tauri/src/commands/{assets,scan,thumbs,import_export}.rs` puste.
- `git diff src-tauri/src/lib.rs` to jedna dodana linia.
- Liczby CRLF bez zmian.

## Ryzyka

- **Kolejność żądań `start_asset_query`.** `begin_request` musi zostać przed `spawn_blocking` i po walidacji.
- **Tekst błędu albo kolejność blokad.** Przenoszone dosłownie; diff `--color-moved`.
- **Blokada przez `.await`.** Use case'y są synchroniczne i wołane tylko w `spawn_blocking` (poza `begin_asset_query_service`, który nie bierze blokad workflowów).
- **Mieszane końce linii** (patrz etapy 04 i 05).

## Status

done (2026-11-14). Commit czeka na akceptację użytkownika.

## Notatki agentów

### 2026-11-14 — agent etapu 06
- Zrobione:
  - `src-tauri/src/usecases/{mod,assets,scan,thumbs,import_export}.rs`: wszystkie `*_service` z `commands/{assets,scan,thumbs,import_export}.rs` przeniesione dosłownie (skrypt składał pliki z zakresów linii `HEAD`, potem porównanie znormalizowanych bloków: wszystkie obecne, poza dwiema sygnaturami zawiniętymi przez `rustfmt` po dodaniu `pub`); ciała inline wydzielone jako `<komenda>_service` (zapytania, `find_duplicate_assets`, miniatury, `scan_startup_roots`);
  - komendy cienkie (5–15 linii, `start_asset_query` ~35 z powodu 9 argumentów wire); `pub use` typów wejściowych w `commands/assets.rs`;
  - `lib.rs`: jedna linia `pub mod usecases;` (zgoda użytkownika); `generate_handler!` bez zmian;
  - reguła 4 bez `#[ignore]`, z tokenem `locks` (zgoda użytkownika); tabele snapshotu wire i allowlisty bez zmian;
  - dokumentacja: `system-overview.md` (diagram, krok 4–5 ścieżki żądania, tabela modułów z nowym wierszem `usecases/*`, reguła 4 w opisie granicy, matryca blokad, checklist, verification map; usunięte ograniczenie „Several command handlers contain filesystem/DB orchestration”), `ipc-contract.md`, `library-query-and-gallery.md` (gdzie odbywa się walidacja i rejestracja żądania), `scanning-and-indexing.md`, `data-safety-and-portability.md`, `database.md`, `thumbnails.md`, `lightbox.md`, `testing.md`, `CLAUDE.md`, reguły `.claude/rules/{backend-core,ipc-contract,data-safety,scanning,thumbnails}.md`, batche audytu B1/B2a/B4b/B5, skill `tauri-rust-tests`; README planu (status, szkic, reguła 4, notatki `Z etapu 06` dla 07, 08 i 09).
- Odkryte:
  - `DatabaseRuntime::admit` bierze `self: &Arc<Self>` (`services/db_pool.rs:63`), więc use case'y zapytań przyjmują `&Arc<DatabaseRuntime>`, nie `&DatabaseRuntime`;
  - zasada przy `apply_selection_mutation` i `cancel_render_all_thumbnails`: komenda może wołać serwis rdzenia, który sam bierze pozwolenie lub nic nie blokuje; pośrednik bez treści nic nie wnosi (decyzja 9);
  - `open_video` (`commands/video.rs:53-58`) sam pobiera połączenie (`database.admit()?.connection()`); reguła 4 nie flaguje `admit`, więc to przechodzi. Zostawione zgodnie z D1, notatka dla 09;
  - testy anulowania z `commands/thumbs.rs` testują serwis, nie komendę; przeniesione do `usecases/thumbs.rs`, bo reguła 4 skanuje też `#[cfg(test)]` (używają `db::`);
  - pułapka powłoki: w zsh `for x in "spec FLAG"; do run.sh $x` nie dzieli słów, więc flaga opt-in nie dotarła i trzy specy przeszły jako `skipped`; uruchomione ponownie z jawnymi argumentami. `tag-editor` wymaga `XDG_DATA_HOME` pod `/tmp/mediatagger-*` (`e2e/specs/tag-editor.e2e.js:166`); profil w scratchpadzie odpada na `before all`.
- Zostało: commit po akceptacji; regeneracja evali `review-changes` i decyzja o `open_video` w etapie 09.
- Testy:
  - `--lib`: przed 364 passed, 2 ignored; po 364 passed, 2 ignored; zbiór nazw (ostatni segment) identyczny, 13 testów zmieniło tylko prefiks ścieżki;
  - `--test architecture_boundaries`: przed 24 passed, 1 ignored; po 25 passed, 0 ignored;
  - `rg 'db::|locks|with_\w*lock|Connection' src-tauri/src/commands/{assets,scan,thumbs,import_export}.rs` → puste; `usecases` w `src` tylko w `lib.rs`, `commands/*`, `usecases/*`;
  - liczby CRLF bez zmian (`thumb_service.rs` 423, `thumb_scheduler.rs` 459, `thumbs.rs` 393, `indexer.rs` 232, `main.rs` 2; edytowane i nowe pliki 0); `rustfmt` uruchomiony tylko na `usecases/mod.rs` (prowadzi do nowych plików LF), czterech plikach komend i `architecture_boundaries.rs`;
  - `bun run rust:fmt` → pass; `bun run rust:clippy` → pass; `bun run test:backend` → pass (lib 364, `architecture_boundaries` 25, `backend_e2e` 1, `backend_integration` 22 + 1 ignored, `backend_runtime` 6 + 1 ignored, `tag_vocabulary` 6, `thumbnail_bulk` 2, `thumbnail_clear` 2; ignorowane diagnostyki bez zmian);
  - `python3 -I -m unittest` (skrypty audytu) → pass (31); `git diff --check` → pass;
  - próby mutacyjne (każda cofnięta skryptem w trybie binarnym z kontrolą `md5`; pliki CRLF nietykane):
    - M1. `use crate::db::AssetMetaFilter;` w `commands/assets.rs` → reguła 4: `src/commands/assets.rs:3: rule 4: db`;
    - M2. `with_scan_lock` + `scan_service::scan_roots` z powrotem w `scan_startup_roots` → reguła 4 (`locks`, `with_scan_lock`);
    - M3. `with_thumb_operation` z powrotem w `render_all_thumbnails` → reguła 4 tylko dzięki nowemu tokenowi `locks` (stara reguła by przepuściła);
    - M3b. `use crate::app::locks::with_database_maintenance;` w `commands/import_export.rs` → reguła 4 (`locks`);
    - M4a. `use crate::commands::assets::AssetMetaFilterInput;` w `usecases/scan.rs` → reguła 3 (`crate::commands`); M4b. `crate::adapter::startup_scan` → reguła 3 (`crate::adapter`);
    - M5. `progress: &tauri::AppHandle` w `find_duplicate_assets_service` → reguła 1 (`tauri`, `AppHandle`);
    - M6. `use crate::usecases;` w `db/query_revisions.rs` → reguła 2 (`crate::usecases`);
    - M7. `pub use` typów wejściowych zamienione na prywatne `use` → `architecture_boundaries` się nie kompiluje (`dto_snapshot`, E0603);
    - E1a. tekst „Folder path does not exist…” w `scan_folder_service` → wszystko zielone (**bez strażnika**); E1b. ten sam tekst w `add_scan_root_service` → `add_scan_root_rejects_invalid_folder_and_does_not_persist_root` czerwony;
    - E2. wartość `LIBRARY_BUSY_ERROR` → zielone (test porównuje ze stałą; Vitest `LightboxModal.test.tsx:507` i `backendText.test.ts` mają własny literał, E2E go nie sprawdza): **bez strażnika** w Rust;
    - E3 `"Invalid query owner ID"`, E4 `"At most 256 asset IDs are allowed"`, E5 `"Invalid hasNoTags meta filter"` → zielone, **bez strażnika** (`rg` w `src-tauri/tests`, `e2e`, `src` bez trafień);
    - L1. zamiana guardów `scan_lock`/`thumb_lock` w `with_scan_and_thumb_lock` (`app/locks.rs`) → cały `cargo test` zielony: **brak testu kolejności blokad**;
    - L2. `try_with_scan_and_thumb_lock` → `with_scan_and_thumb_lock` w `delete_asset_service` → `delete_fails_fast_while_a_scan_holds_the_scan_lock_and_changes_nothing` wisi (zakleszczenie, przerwane `timeout 60`); czerwone, ale jako zawieszenie, nie porażka;
    - R1. `begin_request` przed walidacją `owner_id` → cały `cargo test` zielony: **kolejność walidacja → rejestracja bez testu Rust** (kolejność względem `spawn_blocking` też tylko w kodzie i E2E);
  - desktop E2E (prywatny Xvfb, tymczasowe `XDG_*`, jeden spec naraz, profile usunięte po przebiegu):
    - `app.smoke` → 4 passing;
    - `app.workflows` → 14 passing;
    - `gallery-selection` (`MEDIATAGGER_GALLERY_SELECTION=1`) → 7 passing;
    - `tag-editor` (`MEDIATAGGER_TAG_EDITOR=1`) → 6 passing;
    - `settings-maintenance` (`MEDIATAGGER_SETTINGS_MAINTENANCE=1`) → 3 passing;
  - `/review-changes HEAD --save` (soczewki conventions, rust, performance; raport w ignorowanym `raports/`): rust i performance bez uwag; conventions 3 kandydatów, weryfikator potwierdził 2 (P3: `testing.md:142` opisywał regułę 4 jako ignorowaną; P3: status 06 w README `done`, a w tym pliku `in progress` bez wyników), odrzucił 1 (`thumbnails.md:199`: test z blokadami istnieje, `thumb_service.rs:1938`; poprawione tylko sformułowanie). Obie potwierdzone uwagi poprawione w tej zmianie.
