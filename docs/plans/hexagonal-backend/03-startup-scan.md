# Etap 03: startup scan do adaptera

Fala B. Warstwa: Rust (stan i orkiestracja startup scanu, jedna komenda, `lib.rs`) i dokumentacja. Kontrakt wire bez zmian.

Dokumentacja do przeczytania:
- [README planu](README.md) (sekcja 03, „Fale i równoległość”, „Zasady dla agentów”);
- [ADR 0034](../../adr/0034-tauri-independent-backend-core.md);
- `docs/architecture/system-overview.md` (startup order, „Managed state”);
- `docs/architecture/ipc-contract.md` (`scan_startup_roots`);
- `docs/subsystems/scanning-and-indexing.md`.

## Cel

Usunąć `app/state.rs` z `FRAMEWORK_ALLOWLIST` w `src-tauri/tests/architecture_boundaries.rs` (5 → 4 wpisy). Stan startup scanu na kanałach `tauri::async_runtime` i jego orkiestracja trafiają do adaptera, a rdzeń zostaje synchroniczny.

## Problemy (potwierdzone w kodzie, commit `9ccc8ba`)

- `app/state.rs:41-108`: `StartupScanState`, `StartupScanPhase`, `StartupScanRequest` i aliasy `StartupScanResult`/`StartupScanReceiver` używają `tauri::async_runtime::{channel, Sender, Receiver}` (`:46,50,82,87`). Reszta pliku (`AppState`, `StartupPopularTags`) jest wolna od Tauri.
- `app/state.rs:194-239`: test `launch_snapshot_only_consumes_enabled_roots_once_under_contention` w `mod startup_tests`, obok czterech testów popular tags, które zostają.
- `commands/scan.rs:193-219`: `run_startup_scan` startuje process-owned zadanie przez `tauri::async_runtime::spawn` + `spawn_blocking` i czeka na kanał.
- `commands/scan.rs:240-343`: helper `startup_state()` i trzy testy `run_startup_scan` (wspólne czekanie z porzuconym obserwatorem, zachowany błąd i panika workera bez ponowienia, null bez rootów).
- Jedyni użytkownicy: `lib.rs:98` (`app.manage(...)`) i komenda `scan_startup_roots` (`commands/scan.rs:177-191`). Cele w `src-tauri/tests` nie używają `StartupScanState`, a `AppState` się nie zmienia, więc literały w `backend_runtime`, `grid_baseline`, `thumbnail_bulk` i `thumbnail_clear` zostają.
- Wszystkie dotykane pliki `.rs` mają końce LF (0 linii CRLF).

## Rdzeń i adapter

- **Rdzeń (bez zmian):**
  - źródło snapshotu: `db::list_scan_root_settings` i model `ScanRoot { path, auto_scan_on_startup }`;
  - blokada: `with_scan_lock` z `app/locks.rs`;
  - synchroniczne skanowanie: `scan_service::scan_roots`;
  - `StartupPopularTags` zostaje w `app/state.rs` (czysta struktura).
- **Adapter, `src/adapter/startup_scan.rs`:**
  - maszyna stanów `Pending(roots) → Running(waiters) → Finished(result)` na kanałach `tauri::async_runtime`;
  - `StartupScanState::new` z filtrem `auto_scan_on_startup` (konstrukcja snapshotu procesu, przeniesiona dosłownie), `subscribe`, `finish`;
  - `run_startup_scan`: process-owned `spawn` + `spawn_blocking`; teksty błędów `startup scan worker failed: …`, `startup scan completion unavailable`, `startup scan lock failed: …` bez zmian.
- **Komenda `scan_startup_roots` (`commands/scan.rs`, adapter):** pobiera stan i podaje closure `with_scan_lock(… scan_service::scan_roots …)`. Closure to use case; przeniesie go etap 06.
- **Odrzucone:** osobna funkcja rdzenia z filtrem rootów albo maszyna stanów generyczna po kanale. To zmiana logiki i sygnatur bez drugiej implementacji. Etap 07 potrzebuje tylko, żeby bootstrap rdzenia zwrócił `Vec<ScanRoot>`, z którego adapter zbuduje `StartupScanState`.

## Kroki

1. `src-tauri/src/adapter/startup_scan.rs`: typy i `impl StartupScanState` z `app/state.rs` oraz `run_startup_scan` z `commands/scan.rs`, dosłownie (tylko ścieżki `crate::app::state::…` → nazwy lokalne, `run_startup_scan` staje się `pub`). Testy przeniesione razem z kodem, nazwy bez zmian.
2. `adapter/mod.rs`: `pub mod startup_scan;`.
3. `app/state.rs`: bez startup scanu i bez Tauri; test kontencji przeniesiony.
4. `commands/scan.rs`: `scan_startup_roots` importuje stan i helper z adaptera; ciało i sygnatura komendy bez zmian.
5. `lib.rs`: jedna instrukcja (`adapter::startup_scan::StartupScanState`; rustfmt łamie ją na 3 linie), ta sama pozycja w `setup`.
6. `FRAMEWORK_ALLOWLIST`: usunięty `app/state.rs` (zostają 4 wpisy).
7. Dokumentacja (lista w [Pliki](#pliki)).

## Pliki

- Nowe:
  - `src-tauri/src/adapter/startup_scan.rs`
  - ten plik
- Zmienione:
  - `src-tauri/src/adapter/mod.rs`
  - `src-tauri/src/app/state.rs`
  - `src-tauri/src/commands/scan.rs`
  - `src-tauri/src/lib.rs` (jedna instrukcja `app.manage`)
  - `src-tauri/tests/architecture_boundaries.rs` (tylko `FRAMEWORK_ALLOWLIST`)
  - `docs/architecture/system-overview.md`: tabela modułów (`app/state.rs`, `adapter/*`), „Managed state”
  - `docs/subsystems/scanning-and-indexing.md`: entry points, akapit o startup scanie, „Relevant existing tests”
  - `docs/development/testing.md`: gdzie są testy natywne startup scanu
  - `.claude/rules/scanning.md`, `.claude/rules/backend-core.md`: `paths:` + `src-tauri/src/adapter/startup_scan.rs`
  - `.claude/skills/audit/references/batches.json`: batch B1 + `adapter/startup_scan.rs`
  - `README.md` (sekcje 06 i 07: `Z etapu 03`) i `NOTES.md` planu
- Bez zmian: `COMMAND_SIGNATURES`, `REGISTERED_COMMANDS`, `dto_snapshot`, nazwy eventów, `ipc-contract.md` (opisuje zachowanie, nie ścieżki), ADR 0034 (Context opisuje stan na `397181f`), `search-tags-and-media-groups.md` (testy popular tags zostają w `app/state.rs`), `Cargo.toml`, `Cargo.lock`, frontend.

## Testy

- Stan wyjściowy (`9ccc8ba`): `--lib` 360 passed, 2 ignored; reguła 4 (`-- --ignored`): `scan.rs` 24, `assets.rs` 21, `thumbs.rs` 6, `import_export.rs` 3.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib -- startup popular_tags`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- Próby mutacyjne (każda cofnięta):
  - `use tauri::Manager;` w `app/state.rs` → reguła 1;
  - ponowny wpis `("app/state.rs", "03")` → `allowlist_entries_are_still_needed`;
  - `finish` bez pętli `try_send` → test wspólnego czekania czerwony;
  - `StartupScanRequest::Finished(_) => return Ok(None)` → testy odtwarzania wyniku i błędu czerwone;
  - `new` bez filtra `auto_scan_on_startup` → test kontencji czerwony;
  - pusty snapshot jako `Pending` → test bez rootów czerwony.
- `python3 -I -m unittest` w `.claude/skills/audit/scripts`.
- `bun run rust:fmt`, `bun run rust:clippy`, `bun run test:backend`, `git diff --check`.
- Desktop E2E (jeden spec naraz): `startup-scan`, potem `startup-scan-remount` (`MEDIATAGGER_STARTUP_REMOUNT=1`).

## Kryteria akceptacji

- `FRAMEWORK_ALLOWLIST` ma 4 wpisy; `allowlist_entries_are_still_needed` i reguła 1 zielone.
- Snapshot wire (komendy, sygnatury, DTO, eventy) bez zmian.
- `rg '\btauri\b' src-tauri/src/app/state.rs` puste.
- Liczba testów `--lib` bez zmian (360), cztery testy startup scanu pod `adapter::startup_scan::tests`.
- `git diff src-tauri/src/lib.rs` to jedna zmieniona instrukcja `app.manage(…)` w tym samym miejscu `setup`.

## Ryzyka

- **Semantyka współdzielonego czekania** (porzucony obserwator nie anuluje pracy, wynik i błąd są zachowane). Kod przeniesiony bez zmian logiki; pokrywają ją przeniesione testy i oba specy E2E.
- `adapter/mod.rs` jest wspólny z etapem 05, który dopisze swój moduł.

## Status

done (2026-11-14). Commit czeka na akceptację użytkownika.

## Notatki agentów

### 2026-11-14 — agent etapu 03
- Zrobione:
  - `adapter/startup_scan.rs`: `StartupScanState`, `StartupScanRequest`, `run_startup_scan` i cztery testy, przeniesione dosłownie (zmienione tylko ścieżki `crate::app::state::…` i `pub` przy `run_startup_scan`; sprawdzone diffem względem `HEAD`);
  - `app/state.rs` bez Tauri (`AppState`, `StartupPopularTags` i testy popular tags zostają);
  - komenda `scan_startup_roots` bez zmian w ciele, importuje stan i helper z adaptera; `lib.rs` zmienia tylko ścieżkę typu w `app.manage(…)`;
  - `FRAMEWORK_ALLOWLIST`: 5 → 4 wpisy;
  - dokumentacja, reguły `scanning`/`backend-core`, batch audytu B1, notatki `Z etapu 03` w sekcjach 06 i 07 README.
- Odkryte:
  - cele `src-tauri/tests` nie używają `StartupScanState`, a `AppState` się nie zmienił, więc literały w `backend_runtime`, `grid_baseline`, `thumbnail_bulk` i `thumbnail_clear` zostają;
  - batch audytu B12 (`src-tauri/src/app/*.rs`) przestał obejmować startup scan; teraz obejmuje go B1;
  - rustfmt łamie nową instrukcję w `lib.rs` na trzy linie (szerokość > 100).
- Zostało: commit po akceptacji.
- Testy:
  - stan wyjściowy i końcowy `--lib`: 360 passed, 2 ignored (bez utraty testów); reguła 4 (`-- --ignored`) bez zmian: `scan.rs` 24, `assets.rs` 21, `thumbs.rs` 6, `import_export.rs` 3;
  - `cargo test … --lib -- startup popular_tags` → pass (20, w tym 4 pod `adapter::startup_scan::tests`);
  - `cargo test … --test architecture_boundaries` → pass (23 passed, 1 ignored);
  - próby mutacyjne (6), wszystkie czerwone zgodnie z oczekiwaniem i cofnięte:
    - `use tauri::Manager;` w `app/state.rs` → `core_does_not_use_the_gui_runtime` (`src/app/state.rs:1: rule 1: tauri`);
    - ponowny wpis `("app/state.rs", "03")` → `allowlist_entries_are_still_needed` („remove it from FRAMEWORK_ALLOWLIST (stage 03 done)”);
    - `finish` bez `try_send` → testy wspólnego czekania i zachowanego błędu czerwone;
    - `Finished(_) => return Ok(None)` → testy odtwarzania wyniku i błędu czerwone;
    - `new` bez filtra `auto_scan_on_startup` → test kontencji czerwony;
    - pusty snapshot jako `Pending` → test bez rootów i test kontencji czerwone;
  - `python3 -I -m unittest` (skrypty audytu) → pass (31);
  - `bun run rust:fmt` → pass; `bun run rust:clippy` → pass;
  - `bun run test:backend` → pass (lib 360, `architecture_boundaries` 23, pozostałe cele zielone);
  - `git diff --check` → pass;
  - desktop E2E (prywatny Xvfb, tymczasowe `XDG_*`, jeden spec naraz): `startup-scan` → 1 passing; `startup-scan-remount` (`MEDIATAGGER_STARTUP_REMOUNT=1`) → 2 passing.
