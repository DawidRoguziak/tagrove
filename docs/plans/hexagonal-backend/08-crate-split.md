# Etap 08: wydzielenie crate'a `media_tagger_core`

Fala D. Warstwa: Rust (workspace Cargo, nowy pakiet rdzenia, ścieżki w adapterze i testach, newtype `TauriProgress`, `architecture_boundaries.rs`), `Cargo.lock`, pakowanie (Dockerfile'e, skrypty) i dokumentacja. Kontrakt wire bez zmian.

Dokumentacja do przeczytania:
- [README planu](README.md) (sekcja 08 z notatkami „Z etapu 01/02/04/05/06/07”, „Egzekucja granicy”, „Zasady dla agentów”);
- [ADR 0034](../../adr/0034-tauri-independent-backend-core.md), [ADR 0028](../../adr/0028-media-tools-open-only-the-file-they-are-given.md), [ADR 0032](../../adr/0032-rusqlite-0-41-for-bundled-sqlite-3-54.md);
- `docs/architecture/system-overview.md`, `docs/development/{setup-and-build,testing,linux-packaging}.md`.

## Cel

Rdzeń backendu jest osobnym pakietem `media_tagger_core` w workspace `src-tauri/`, a adapter `media_tagger` importuje go po ścieżce. Granicę rdzeń/adapter pilnuje kompilator: rdzeń nie ma zależności GUI i nie widzi adaptera. Zachowanie, kontrakt wire, profile i dane aplikacji bez zmian.

## Decyzje

1. **Workspace** w `src-tauri/Cargo.toml`: `members` i `default-members` = `[".", "crates/media_tagger_core"]`, `resolver = "2"`. `[workspace.dependencies]`: `anyhow`, `image`, `libc`, `rusqlite` (z komentarzem o ADR 0032), `serde`, `serde_json`, `tempfile`. Dzięki `default-members` każde `cargo … --manifest-path src-tauri/Cargo.toml` (skrypty `test:backend`, `rust:clippy`, `rust:fmt`) obejmuje oba pakiety bez zmian w `package.json`.
2. **Adapter `media_tagger`:** `[[bin]]`, `build.rs`, `custom-protocol` i zależności GTK/libmpv bez zmian. Zależności: `tauri`, oba pluginy, `anyhow`, `libc`, `serde`, `serde_json`, `media_tagger_core` (path). Dev: `image`, `rusqlite`, `tempfile`, `media_tagger_core` z `test-support`.
3. **Rdzeń:** `anyhow`, `csv`, `fs2`, `image`, `jpeg-decoder`, `libc`, `rusqlite`, `rand`, `serde`, `serde_json`, `sha2`, `walkdir`, `zip`; dev `tempfile`. `publish = false`, `[features] test-support = []`.
4. **Moduły:** `lib.rs` rdzenia ma te same `pub mod` co wcześniej adapter (bez `adapter`, `commands`, `video`) plus `#[cfg(any(test, feature = "test-support"))] pub mod test_support;`. W `test_support.rs` tylko `pub(crate)` → `pub`. `app/mod.rs` rdzenia traci `profile`; adapter ma nowy `src/app/mod.rs` z samym `profile`, a `lib.rs` deklaruje tylko `adapter`, `app`, `commands`, `video`.
5. **Ścieżki w adapterze:** `crate::{models, db, services, usecases, app::state, error, test_support}` → `media_tagger_core::…`, bez re-eksportów. `lib.rs` dostaje `use media_tagger_core::{app::{bootstrap, instance_lock::InstanceLock}, services};`, więc linia rejestracji `thumb://` jest bajtowo ta sama, a zmieniają się tylko trzy wywołania `app::bootstrap::` → `bootstrap::`.
6. **Reguła sierot:** `pub struct TauriProgress<'a, R: Runtime>(pub &'a AppHandle<R>)` w `adapter/events.rs` z tym samym ciałem `progress`; w 8 komendach (`scan_folder`, `rescan_all_roots`, `scan_startup_roots`, `find_duplicate_assets`, `render_all_thumbnails`, `render_failed_thumbnails`, `clear_all_thumbnails`, `clear_library_data`) `&app` → `&TauriProgress(&app)`.
7. **Porty zostają w obecnych plikach**, bez `ports.rs`; szkic w README poprawiony.
8. **`architecture_boundaries.rs`:** `SourceFile` zna korzeń (`Root::Core`/`Root::Adapter`); usunięte reguły 1 i 3, obie allowlisty, `ADAPTER_FIXED`, `allowlist_entries_*`; nowy `core_manifest_has_no_gui_dependencies` (parser manifestu, tylko std) z autotestem `manifest_parser_finds_every_dependency_shape` na syntetycznym manifeście; zostają reguła 2 (`db` rdzenia, zakaz `services`/`app`/`usecases`), reguła 4 (`commands/` adaptera), `video_does_not_import_commands`, komendy, sygnatury, eventy i `dto_snapshot` (komendy i wideo z `media_tagger`, modele i serwisy z `media_tagger_core`). Testy skanera przepisane bez `framework_hits` (`scanner_finds_framework_identifiers` → `scanner_finds_identifiers_with_lines`).
9. **Testy w `src-tauri/tests`:** `media_tagger::` → `media_tagger_core::`; w `backend_e2e.rs` (CRLF) jedna linia zmieniona binarnie (`sed`), bez `rustfmt`.
10. **Pakowanie:** `Dockerfile.linux` i `Dockerfile.native` kopiują manifest rdzenia i tworzą stub jego `lib.rs` w warstwie `cargo fetch`; `privacy-check.py` dostaje klucz `EXAMPLES` dla nowej ścieżki `backup_service.rs` (stary zostaje dla historii); `notices.py` kopiuje manifest rdzenia jako `media_tagger_core.Cargo.toml`; `verify-release.py` wymaga manifestu rdzenia. Sprawdzone i bez zmian: `prepare-source.py` (`git archive` całego commita), `source-info.sh` (hash `Cargo.lock`), `flatpak/generate.py` (pomija wpisy bez `source`, więc lokalny pakiet nie trafia do źródeł), `e2e/*` (workspace ma korzeń w `src-tauri`, więc `target-e2e/debug/media_tagger` bez zmian), `package.json`.
11. **`.claude/**` bez zmian** (globy reguł, `HOT_PATHS`, `batches.json`): etap 09, notatka „Z etapu 08” w README.
12. Docker i Flatpak nie są uruchamiane; zamiast tego `cargo fetch --locked --offline` na kopii manifestów ze stubami w scratchpadzie.
13. **`Cargo.lock`:** diff tylko w lokalnych pakietach (patrz [Testy](#testy)). Od tego miejsca Cargo tylko z `--locked`.

Odrzucone:
- re-eksporty (`pub use media_tagger_core::*`) w adapterze: zacierają granicę, którą ma pokazywać kompilator;
- `impl ProgressSink` przez blanket impl dla closure `Fn(ScanProgress)`: zmienia API use case'ów; newtype jest lokalny i nie zmienia sygnatur rdzenia;
- `ports.rs`: przenosi kod bez zysku dla granicy;
- kopia fixtur ADR 0028 w adapterze (dwie wersje testu bezpieczeństwa).

## Kroki

1. Manifesty, `lib.rs` rdzenia, `app/mod.rs` po obu stronach, ścieżki w adapterze i testach, `TauriProgress`, `test_support`.
2. `Cargo.lock` i kontrola diffu.
3. `cargo build -p media_tagger_core --locked`; `cargo tree -p media_tagger_core --locked -e normal`; `cargo test --locked`.
4. `architecture_boundaries.rs`; `rustfmt` tylko na edytowanych plikach LF.
5. `bun run quality`, `privacy:check`, `test:privacy`, `test:packaging`; `git diff --check`; liczby CRLF i md5.
6. Porównanie list testów z bazą per cel.
7. Próby mutacyjne.
8. Dokumentacja.
9. Desktop E2E i `tauri:dev` na prywatnym Xvfb.
10. `/review-changes HEAD --save`.

## Pliki

- Nowe:
  - `src-tauri/crates/media_tagger_core/Cargo.toml`, `src-tauri/crates/media_tagger_core/src/lib.rs`;
  - `src-tauri/src/app/mod.rs` (sam `profile`);
  - ten plik.
- Przeniesione (`git mv`, 42 rename'y, treść bez zmian poza dwoma plikami): `app/{bootstrap,instance_lock,locks,mod,state}.rs`, `db.rs`, `db/*`, `error.rs`, `indexer.rs`, `models.rs`, `services/*`, `test_support.rs`, `thumbs.rs`, `usecases/*`, `utils/*` → `src-tauri/crates/media_tagger_core/src/`. Zmienione po przeniesieniu: `app/mod.rs` (bez `profile`), `test_support.rs` (`pub(crate)` → `pub`, `rustfmt` złamał linie).
- Zmienione:
  - `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`;
  - `src-tauri/src/lib.rs`, `src-tauri/src/adapter/{events,startup_scan,thumb_scheme}.rs`, `src-tauri/src/commands/{assets,import_export,scan,thumbs,video}.rs`, `src-tauri/src/video/player.rs` (tylko import `test_support`);
  - `src-tauri/tests/*.rs` i `tests/support/assets.rs` (importy), `tests/architecture_boundaries.rs` (reguły);
  - `Dockerfile.linux`, `Dockerfile.native`, `scripts/privacy-check.py`, `packaging/linux/notices.py`, `packaging/flatpak/verify-release.py`;
  - `docs/architecture/system-overview.md`, `docs/development/{setup-and-build,testing,linux-packaging}.md`, `CLAUDE.md` („Naming”, „Layout”, „Commands”);
  - `docs/architecture/ipc-contract.md` (jedno zdanie o `ProgressSink`, poprawka z review);
  - ADR 0034 (tylko Decision i Consequences; status Proposed);
  - `README.md` i `NOTES.md` planu.
- Bez zmian: `generate_handler!`, rejestracja `thumb://`, strażnik profilu, `COMMAND_SIGNATURES`, `REGISTERED_COMMANDS`, JSON w `dto_snapshot`, eventy, frontend, `package.json`, `e2e/*`, `.claude/**`, `src-tauri/src/main.rs`.

## Testy

Opisane w [Notatkach agentów](#notatki-agentów).

## Kryteria akceptacji

- `cargo tree` rdzenia nie zawiera GUI, a `cargo build -p media_tagger_core --locked` przechodzi.
- `architecture_boundaries` zielony, tabele snapshotu bez zmian.
- Liczba testów nie spadła poza udokumentowanymi wyjątkami.
- `fmt`, `clippy`, `test:backend`, `quality` i `privacy:check` zielone.
- Pliki rdzenia w diffie jako rename'y; liczby CRLF i md5 zgodne (poza jedną linią w `backend_e2e.rs`).
- E2E i `tauri:dev` zielone.

## Ryzyka

- **`Cargo.lock`.** Tylko lokalne wpisy; każda inna różnica = cofnięcie i stop.
- **Cache warstw Dockera.** Stub `lib.rs` rdzenia jest potrzebny, bo `cargo fetch` czyta manifest członka workspace.
- **Mieszane końce linii.** Pliki CRLF przeniesione bez treściowej zmiany; jedyna edycja to linia importu w `backend_e2e.rs`.

## Status

done (2026-11-14). Commit czeka na akceptację użytkownika.

## Notatki agentów

### 2026-11-14 — agent etapu 08
- Zrobione: decyzje 1–13 jak wyżej.
- Odkryte:
  - `cargo metadata --offline` (planowany sposób odświeżenia `Cargo.lock`) chce pobrać źródła wszystkich platform (`android_system_properties`) i offline się nie udaje; użyte `cargo update --workspace --offline`, które dotyka tylko pakietów workspace'u ("Locking 0 packages");
  - z tego samego powodu `cargo fetch --locked --offline` bez `--target` nie przechodzi ani na nowych manifestach, ani na manifestach z HEAD (`windows-sys`); kontrola Dockerfile'a idzie z `--target x86_64-unknown-linux-gnu`;
  - `--test <cel>` działa bez `-p` przy dwóch pakietach (cele integracyjne ma tylko adapter), więc skrypty i dokumentacja nie potrzebowały `-p media_tagger`;
  - `rustfmt` na edytowanych plikach zmienił tylko `test_support.rs`, `architecture_boundaries.rs` i `grid_baseline.rs`; `cargo fmt --all -- --check` czysty bez dotykania plików CRLF.
- Zostało: commit po akceptacji; `.claude/**` i ścieżki w stronach podsystemów (notatka „Z etapu 08” w README) w etapie 09.
- Testy:
  - `Cargo.lock`: `media_tagger` traci `csv`, `fs2`, `jpeg-decoder`, `rand 0.8.8`, `sha2`, `walkdir`, `zip`, zyskuje `media_tagger_core`; nowy wpis `media_tagger_core` (`anyhow`, `csv`, `fs2`, `image`, `jpeg-decoder`, `libc`, `rand 0.8.8`, `rusqlite`, `serde`, `serde_json`, `sha2`, `tempfile`, `walkdir`, `zip`); innych różnic brak;
  - `cargo build -p media_tagger_core --locked` → pass; `cargo tree -p media_tagger_core --locked -e normal --prefix none` (bez ścieżek) → brak `tauri`, `gtk`, `webkit`, `mpv`, `glib`, `gdk`, `wry`, `tao` (150 linii);
  - lista testów (`--list`, w tym ignorowane) per cel, przed → po: lib `media_tagger` 373 → `media_tagger` 31 + `media_tagger_core` 342 = 373, zbiór nazw identyczny; `architecture_boundaries` 25 → 23 (usunięte `core_does_not_use_the_gui_runtime`, `core_does_not_import_the_adapter`, `allowlist_entries_exist`, `allowlist_entries_are_still_needed`; przemianowany `scanner_finds_framework_identifiers` → `scanner_finds_identifiers_with_lines`; nowe `core_manifest_has_no_gui_dependencies`, `manifest_parser_finds_every_dependency_shape`); pozostałe cele bez zmian (`backend_e2e` 1, `backend_integration` 23, `backend_runtime` 7, `grid_baseline` 6, `tag_save_benchmark` 1, `tag_vocabulary` 6, `thumbnail_bulk` 2, `thumbnail_clear` 2); razem 446 → 444;
  - `bun run test:backend` (`cargo test --locked`) → pass: `media_tagger` lib 31, `media_tagger_core` lib 340 + 2 ignored, `architecture_boundaries` 23, `backend_e2e` 1, `backend_integration` 22 + 1 ignored, `backend_runtime` 6 + 1 ignored, `grid_baseline` 6 ignored, `tag_save_benchmark` 1 ignored, `tag_vocabulary` 6, `thumbnail_bulk` 2, `thumbnail_clear` 2;
  - `bun run quality` (locale, typecheck, lint, format, `rust:fmt`, `rust:clippy -D warnings` na obu pakietach, audyty) → pass; `bun run privacy:check` → pass (243 commity); `bun run test:privacy` → pass; `bun run test:packaging` → pass;
  - `git diff --check`: jedyne zgłoszenie to zmieniona linia CRLF w `tests/backend_e2e.rs` (`\r`), patrz notatka „Z etapu 08”;
  - liczby CRLF i md5 (po przeniesieniu): `services/thumb_service.rs` 423 `fe57052…`, `services/thumb_scheduler.rs` 459 `e6fe845…`, `thumbs.rs` 393 `b2afad9…`, `indexer.rs` 232 `035d983…`, `main.rs` 2 `48bce40…` — identyczne z bazą; `tests/backend_e2e.rs` 130 CRLF (bez zmian), md5 inne o jedną linię importu;
  - Dockerfile (`cargo fetch --manifest-path src-tauri/Cargo.toml --locked --offline --target x86_64-unknown-linux-gnu` na kopii samych manifestów i `Cargo.lock` ze stubami `lib.rs`) → pass, `Cargo.lock` bez zmian; ten sam fetch bez manifestu rdzenia → błąd (stub potrzebny); ze starym `Cargo.lock` → „cannot update the lock file … --locked”;
  - próby mutacyjne (skrypt w trybie binarnym, każda cofnięta z kontrolą `md5`; po wszystkich md5 101 plików `*.rs`/`Cargo.*` zgodne ze zrzutem sprzed prób; skan źródeł przez zbudowaną binarkę `architecture_boundaries`, bez rebuildu):
    - `tauri = "2.11.5"` w `[dependencies]` rdzenia → `cargo build -p media_tagger_core --locked --offline`: „cannot update the lock file”; `core_manifest_has_no_gui_dependencies` czerwony (`["tauri"]`);
    - `use tauri::Manager;` w `lib.rs` rdzenia → E0432 `unresolved import tauri`;
    - `use crate::adapter::events::TauriProgress;` w rdzeniu → E0433; `use media_tagger::commands;` → E0432;
    - `use crate::services;` w `db/schema.rs` → reguła 2 (`crates/media_tagger_core/src/db/schema.rs:1520: rule 2: crate::services`);
    - `media_tagger_core::db::init_schema` w `commands/window.rs` → reguła 4 (`src/commands/window.rs:55: rule 4: db`);
    - bez `impl ProgressSink for TauriProgress` → 8× E0277 (jedno na komendę); impl przywrócony na `AppHandle<R>` → E0117 (+ E0277);
    - `use crate::commands;` w `video/types.rs` → `video_does_not_import_commands` czerwony;
    - dev-dependency rdzenia bez `test-support` → `cargo test -p media_tagger --lib --no-run`: E0432 `media_tagger_core::test_support`;
    - `default-members = ["."]` → `cargo test -- --list` 102 testy zamiast 444 (znika 342 testów rdzenia);
    - `"process-progres"` w `adapter/events.rs` → `emitted_event_names_are_frozen` czerwony.
  - desktop E2E (prywatny Xvfb, tymczasowe `XDG_*` pod `/tmp/mediatagger-*`, `MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"`, jeden spec naraz, każdy wywołany jawnie w skrypcie bash; profile usunięte po przebiegu): `bun run tauri:build:e2e` → pass; `app.smoke` → 4 passing; `startup-scan` → 1 passing; `thumbnail-fade` (`MEDIATAGGER_THUMBNAIL_FADE=1`) → 2 passing; `settings-maintenance` (`MEDIATAGGER_SETTINGS_MAINTENANCE=1`) → 3 passing; `native-video-escape` (`MEDIATAGGER_NATIVE_VIDEO=1`) → 8 passing;
  - `bun run tauri:dev` na prywatnym Xvfb z tymczasowym profilem dev, dwa starty pod własnym `setsid`: (1) świeży profil → `Running target/debug/media_tagger`, w profilu `instance.lock`, `media.db`, `thumbs/`; (2) po dopisaniu rootu z `auto_scan_on_startup=1` → skan startowy, galeria „Tagrove” z 6 miniaturami, 6 assetów w bazie; w logach tylko ostrzeżenia środowiska i SIGTERM z zamknięcia; zamknięcie przez `kill -TERM` grupy procesów, port 1420 wolny, katalog tymczasowy usunięty. Workspace z dwoma `default-members` nie przeszkadza ani `tauri dev` (`cargo run --no-default-features`), ani `tauri:build:e2e`;
  - `/review-changes HEAD --save` (soczewki conventions, rust, performance; raport w ignorowanym `raports/`): rust i performance bez uwag; conventions 1 kandydat, weryfikator potwierdził (P3: `ipc-contract.md` mówił, że komenda podaje `AppHandle` jako sink, co po usunięciu `impl ProgressSink for AppHandle` jest nieprawdą). Poprawione w tej zmianie: zdanie opisuje `TauriProgress` i `&TauriProgress(&app)`, ścieżka `progress.rs` wskazuje crate rdzenia; po poprawce `--test architecture_boundaries` zielony (test `registered_commands_are_documented` czyta ten plik).
