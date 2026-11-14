# Etap 02: postęp i testy bez Tauri

Fala B. Warstwa: Rust (adapter eventów, jedna komenda, testy miniatur) i dokumentacja. Kontrakt wire bez zmian.

Dokumentacja do przeczytania:
- [README planu](README.md) (sekcja 02, „Zasady dla agentów”);
- [ADR 0034](../../adr/0034-tauri-independent-backend-core.md);
- `docs/architecture/system-overview.md` (granice modułów);
- `docs/architecture/ipc-contract.md` („Broadcast progress”).

## Cel

Usunąć dwa wpisy z `FRAMEWORK_ALLOWLIST` w `src-tauri/tests/architecture_boundaries.rs`:
- `services/progress.rs`: port `ProgressSink` zostaje w rdzeniu, a jego implementacja dla Tauri trafia do adaptera;
- `services/thumb_service.rs`: test przestaje używać `tauri::http`, bez utraty pokrycia.

## Problemy (potwierdzone w kodzie, commit `df658da`)

- `services/progress.rs:2,7-12`: `use tauri::Emitter;` i `impl<R: tauri::Runtime> ProgressSink for tauri::AppHandle<R>`, jedyne miejsce emisji `process-progress`.
- `commands/import_export.rs:69-72`: `clear_library_data_service(state: &AppState, app: tauri::AppHandle)` bierze konkretny `AppHandle`, choć używa go tylko jako sinka postępu (dwa `emit_progress` i `delete_thumbnail_files_with_progress`, które już przyjmuje `&impl ProgressSink`).
- `services/thumb_service.rs:3155-3169`: test `legacy_thumbnail_of_an_unreachable_source_is_still_served` buduje odpowiedź `thumb_protocol::image_response` i sprawdza `tauri::http::header::CACHE_CONTROL == "no-store"`. Łańcuch ma dwa ogniwa:
  - serwis podaje pusty token wersji dla zachowanego pliku legacy (`thumb_service.rs:923-930`);
  - protokół mapuje niezgodną wersję na `no-store` (`thumb_protocol.rs:123-140`).
- `adapter/` jest już w `ADAPTER_FIXED` (etap 01); `lib.rs` nie deklarował modułu `adapter`.

## Kroki

1. `src-tauri/src/adapter/{mod.rs,events.rs}`: impl `ProgressSink` dla `AppHandle` przeniesiony dosłownie; `pub mod adapter;` w `lib.rs` (jedyna zmiana w `lib.rs`).
2. `services/progress.rs`: bez impl i bez importu `tauri`. Trait, blanket impl dla `Fn` i `emit_progress` bez zmian.
3. `clear_library_data_service(state: &AppState, progress: &impl ProgressSink)`; komenda podaje `&app`. Ciało dosłownie, poza nazwą sinka.
4. Test `thumb_service` sprawdza inwariant rdzenia: `served.version != version_token(current)`. Nowy test `thumb_protocol::legacy_file_with_empty_version_is_not_cached` sprawdza, że pusty token daje `no-store`. Razem pokrywają cały łańcuch.
5. `FRAMEWORK_ALLOWLIST`: usunięte `services/progress.rs` i `services/thumb_service.rs` (zostaje 5 wpisów).
6. Dokumentacja (lista w [Pliki](#pliki)).

## Pliki

- Nowe:
  - `src-tauri/src/adapter/mod.rs`
  - `src-tauri/src/adapter/events.rs`
  - ten plik
- Zmienione:
  - `src-tauri/src/lib.rs` (`pub mod adapter;`)
  - `src-tauri/src/services/progress.rs`
  - `src-tauri/src/commands/import_export.rs`
  - `src-tauri/src/services/thumb_service.rs` (tylko test; plik ma mieszane końce linii CRLF/LF, edycja musi je zachować)
  - `src-tauri/src/services/thumb_protocol.rs` (nowy test)
  - `src-tauri/tests/architecture_boundaries.rs` (tylko `FRAMEWORK_ALLOWLIST`)
  - `docs/architecture/system-overview.md`: wiersz `adapter/*` w tabeli modułów, port postępu w wierszu `services/*`
  - `docs/architecture/ipc-contract.md`: „Broadcast progress”, kto emituje event
  - `.claude/rules/ipc-contract.md`: `paths:` + `src-tauri/src/adapter/events.rs`
  - `.claude/skills/audit/references/batches.json`: batch B1 + `adapter/events.rs`
  - `README.md` i `NOTES.md` planu
- Bez zmian: `COMMAND_SIGNATURES`, `REGISTERED_COMMANDS`, `dto_snapshot`, nazwy eventów, ADR 0034 (Context opisuje stan na `397181f`), `Cargo.toml`, `Cargo.lock`, frontend.

## Testy

- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib -- legacy_thumbnail_of_an_unreachable_source thumb_protocol`
- Próby mutacyjne (każda cofnięta):
  - `use tauri::Emitter;` w `services/progress.rs` → `core_does_not_use_the_gui_runtime` (`src/services/progress.rs:1: rule 1`);
  - `==` → `!=` w `image_response` → oba testy wersji w `thumb_protocol` czerwone;
  - plik legacy dostaje `version_token(&item.version)` zamiast pustego tokenu → test `thumb_service` czerwony.
- `python3 -I -m unittest` w `.claude/skills/audit/scripts`.
- `bun run rust:fmt`, `bun run rust:clippy`, `bun run test:backend`.
- Desktop E2E: `settings-maintenance` (spec woła `clear_library_data`).

## Kryteria akceptacji

- `FRAMEWORK_ALLOWLIST` ma 5 wpisów; `allowlist_entries_are_still_needed` i reguła 1 zielone.
- Snapshot wire (komendy, sygnatury, DTO, eventy) bez zmian.
- `rg '\btauri\b' src-tauri/src/services/{progress,thumb_service}.rs` pokazuje tylko literał `"tauri://localhost"`.

## Ryzyka

- **Reguła sierot w etapie 08.** Po wydzieleniu `media_tagger_core` trait `ProgressSink` i typ `AppHandle` będą obce dla crate'a adaptera, więc `impl ProgressSink for AppHandle` przestanie się kompilować (E0117). Zapisane w sekcji 08 README.
- `adapter/mod.rs` jest wspólny dla etapów 03 i 05, które dopiszą do niego swoje moduły.

## Status

done (2026-11-14). Commit czeka na akceptację użytkownika.

## Notatki agentów

### 2026-11-14 — agent etapu 02
- Zrobione:
  - `adapter/{mod,events}.rs` z impl `ProgressSink` dla `AppHandle`;
  - `clear_library_data_service` na `&impl ProgressSink`;
  - asercja `no-store` przeniesiona do `thumb_protocol` (nowy test), a test `thumb_service` sprawdza niezgodny token;
  - dwa wpisy mniej w `FRAMEWORK_ALLOWLIST` (zostaje 5);
  - dokumentacja, reguła IPC, batch audytu B1.
- Odkryte:
  - `thumb_service.rs` ma mieszane końce linii (423 linie CRLF na `df658da`). Edycja przez `read_text`/`write_text` Pythona zamienia cały plik na LF; edytuj bajtowo albo narzędziem, które zachowuje CRLF;
  - po etapie 02 jedynym miejscem `.emit(` w `src-tauri/src` jest `adapter/events.rs`;
  - reguła sierot w etapie 08 (zapisane w README, sekcja 08).
- Zostało: commit po akceptacji.
- Testy:
  - `cargo test … --test architecture_boundaries` → pass (23 passed, 1 ignored);
  - `cargo test … --lib -- legacy_thumbnail_of_an_unreachable_source thumb_protocol` → pass (7);
  - próby mutacyjne (3) → wszystkie czerwone zgodnie z oczekiwaniem, cofnięte;
  - `python3 -I -m unittest` (skrypty audytu) → pass (31);
  - `bun run rust:fmt` → pass; `bun run rust:clippy` → pass;
  - `bun run test:backend` → pass (lib 360, `architecture_boundaries` 23, pozostałe cele zielone);
  - desktop E2E `settings-maintenance` (`MEDIATAGGER_SETTINGS_MAINTENANCE=1`, prywatny Xvfb, tymczasowe `XDG_*`) → 3 passing.
