# Etap 07: bootstrap w rdzeniu

Fala C. Warstwa: Rust (nowy moduł rdzenia `app/bootstrap.rs`, closure `setup` w `lib.rs`, komentarze w dwóch testach) i dokumentacja. Kontrakt wire bez zmian.

Dokumentacja do przeczytania:
- [README planu](README.md) (sekcja 07 z notatkami „Z etapu 03”, „Z etapu 04”, „Z etapu 05” i „Z etapu 06”, „Fale i równoległość”, „Zasady dla agentów”, „Egzekucja granicy”);
- [ADR 0034](../../adr/0034-tauri-independent-backend-core.md), [ADR 0006](../../adr/0006-thumb-uri-scheme.md), [ADR 0016](../../adr/0016-startup-sweep-of-abandoned-backup-temporaries.md);
- `docs/architecture/system-overview.md` (startup order, managed state, tabela modułów, profile i izolacja app-data);
- `docs/subsystems/data-safety-and-portability.md` (odzysk restore, sprzątanie temporariów backupu, odzysk oczekujących operacji plikowych);
- `docs/subsystems/thumbnails.md` (scheduler, okno 503 `thumb://`, odkrywanie ffmpeg);
- `docs/development/setup-and-build.md`, `docs/development/testing.md`;
- `.claude/rules/backend-core.md`, `database.md`, `data-safety.md`, `thumbnails.md`.

## Cel

`lib.rs` zawiera tylko składanie aplikacji: builder, pluginy, rejestrację `thumb://`, strażnik identyfikatora, ścieżki Tauri, `InstanceLock`, fazę wideo, `manage()` i `generate_handler!`. Fazy rdzenia startu (odzysk, sprzątanie, otwarcie bazy, snapshoty, ffmpeg, scheduler, `AppState`) żyją w `app/bootstrap.rs` bez Tauri, a nowy test rdzenia uruchamia je na katalogu tymczasowym. Kolejność startu, teksty błędów i semantyka porażek zostają bez zmian.

## Problemy (potwierdzone w kodzie, commit `8322574`)

- `lib.rs:67-148` closure `setup` miesza fazy:
  - adapter: strażnik identyfikatora (`:68-75`), `app_data_dir` + `create_dir_all` (`:77-81`), `InstanceLock::acquire` + `manage` (`:82-83`), `resource_dir` (`:105-108`), `LC_NUMERIC` i wideo (`:110-121`), wszystkie `manage`;
  - rdzeń: `recover_interrupted_restore` → `remove_abandoned_backup_temporaries` (`:85-86`), `thumbs/` (`:88-89`), `open_connection` + `init_schema` + `synchronous=FULL` (`:91-94`), `recover_pending_file_operations` (`:95`), `list_popular_tags` z logowaniem błędu (`:96-99`), `list_scan_root_settings` (`:101-103`), `resolve_ffmpeg_path` (`:109`), scheduler i kontrola `worker_spawn_failures` (`:122-133`), literał `AppState` (`:135-146`).
- `resolve_ffmpeg_path` i helpery (`lib.rs:200-254`) są wolne od Tauri: `std::env::current_exe`, `Path::metadata`, `PermissionsExt`.
- `resource_dir` jest rozwiązywany **po** snapshotach (`:105`), a jego porażka kończy start po odzysku i otwarciu bazy.
- Połączenie startowe `conn` żyje do końca closure, czyli do po `manage(AppState)`.
- Testy, które odtwarzają start ręcznie i powołują się na `lib.rs`: `backup_service.rs:3045` (`start_profile`), `backend_integration.rs:1683` (`restart_and_recover`) i `:1967` (`restart`). Potrzebują połączenia także przy błędzie odzysku operacji plikowych, więc nie mogą wołać funkcji, która przerywa na `?`.
- Brak testu precedencji wyszukiwania ffmpeg (`system-overview.md:222`).
- Końce linii: `lib.rs`, `app/*.rs`, `backup_service.rs` i `backend_integration.rs` mają LF (0 linii CRLF). `lib.rs` deklaruje moduły prowadzące do plików CRLF (`thumbs.rs`, `indexer.rs`, przez `services` do `thumb_service.rs` i `thumb_scheduler.rs`), więc `rustfmt` nie jest na nim uruchamiany; `app/mod.rs` prowadzi tylko do plików LF.

## Decyzje

1. **Moduł:** `src-tauri/src/app/bootstrap.rs` (rdzeń: poza `ADAPTER_FIXED`, więc reguły 1 i 3 go obejmują) i `pub mod bootstrap;` w `app/mod.rs`.
2. **Sygnatury:**

   ```rust
   pub type BootstrapError = Box<dyn std::error::Error>;
   pub type BootstrapResult<T> = Result<T, BootstrapError>;

   pub struct Storage { db_path: PathBuf, thumbs_dir: PathBuf, connection: rusqlite::Connection }
   pub struct StartupSnapshots { pub popular_tags: StartupPopularTags, pub scan_roots: Vec<ScanRoot> }

   pub fn open_storage(app_data_dir: &Path) -> BootstrapResult<(Storage, StartupSnapshots)>;
   pub fn resolve_ffmpeg_path(resource_dir: &Path) -> PathBuf;
   pub fn start_workers(storage: &Storage, ffmpeg_path: PathBuf) -> BootstrapResult<AppState>;
   ```

   - `open_storage` przyjmuje sam katalog profilu (adapter go rozwiązuje i tworzy przed `InstanceLock`); struktura ścieżek byłaby jednopolowa.
   - `BootstrapError` to dokładnie typ błędu closure `setup` w Tauri 2. Każdy krok robi to samo `?` co dziś (ta sama konwersja `io::Error`/`anyhow::Error`/`AppError`/`rusqlite::Error`/`String` do `Box<dyn Error>`), a `?` w adapterze to tożsamość. `Display` i `Debug` porażki w `.expect("error while running tauri app")` się nie zmieniają.
   - `Storage` ma pola prywatne i trzyma połączenie startowe. `start_workers` je **pożycza**, więc adapter upuszcza `storage` na końcu `setup`, po `manage(AppState)`, jak dziś `conn`. Koszt: dwa klony `PathBuf`.
3. **Snapshoty:** `open_storage` zwraca `StartupPopularTags` (typ rdzenia z `app/state.rs`) i `Vec<ScanRoot>` z `db::list_scan_root_settings` (notatka „Z etapu 03”). Adapter robi `manage(StartupPopularTags)` i `manage(StartupScanState::new(scan_roots))` w tym samym miejscu kolejności startu. `StartupScanState` zostaje typem adaptera.
4. **ffmpeg (decyzja użytkownika):** `resolve_ffmpeg_path` z helperami przechodzi dosłownie do `app/bootstrap.rs` jako osobny krok rdzenia. `open_storage` kończy się na snapshotach; adapter rozwiązuje `resource_dir` w tym samym miejscu co dziś i woła `app::bootstrap::resolve_ffmpeg_path(&resource_dir)`. Odchylenie od szkicu README („`open_storage` … → `resolve_ffmpeg_path`”) jest celowe: zachowuje kolejność porażek przy `resource_dir`.
5. **`start_workers`:** dosłownie `ThumbnailScheduler::from_available_parallelism(ffmpeg_path.clone())`, kontrola `worker_spawn_failures` z tym samym tekstem i literał `AppState`. Zwraca gotowy `AppState`, ale `manage` robi adapter jako ostatni krok `setup` (okno 503, notatka „Z etapu 05”). Bootstrap nie trzyma `AppState` w statyku ani kanale.
6. **Granice:** bootstrap nie importuje `adapter`, `commands`, `video` (reguła 3) ani `usecases` (notatka „Z etapu 06”), nie dotyka wideo ani `LC_NUMERIC` (notatka „Z etapu 04”). Faza wideo zostaje w `setup` między `resolve_ffmpeg_path` a `start_workers`, w tej samej kolejności.
7. **Kolejność `manage` przy porażce:** dziś `StartupPopularTags` trafia do `manage` przed `list_scan_root_settings`; teraz oba snapshoty powstają w `open_storage`, a `manage` następuje po nich. Różnica dotyczy tylko nieudanego startu, który i tak kończy proces. Poza tym kolejność kroków, teksty i czas życia połączenia startowego bez zmian.
8. **Testy odtwarzające start** (`backup_service.rs:3045`, `backend_integration.rs:1683,1967`) zostają; zmieniam tylko ich komentarze (`lib.rs` setup → `app::bootstrap::open_storage`).

Odrzucone:
- `resource_dir` w strukturze ścieżek `open_storage` (zgodne ze szkicem, ale porażka `resource_dir` przesunęłaby się przed odzysk i otwarcie bazy) albo callback `resolve_resource_dir` w API rdzenia.
- `anyhow::Error` albo własny enum błędu bootstrapu: zmienia `Debug` porażki startu.
- `start_workers(storage: Storage)` konsumujące `Storage`: połączenie startowe (ostatnie połączenie WAL, checkpoint przy zamknięciu) zamknęłoby się przed `manage(AppState)`, wewnątrz okna 503.
- `open_storage`/`start_workers` robiące `manage` albo udostępniające `AppState` inną drogą: łamie okno 503.
- `resolve_ffmpeg_path` w `thumbs.rs`: plik z mieszanymi końcami linii (393 CRLF); jedynym wywołującym jest start.
- Przepięcie testów odtwarzających start na `open_storage` (decyzja 8).

## Kroki

1. Ten plik; status 07 w README i NOTES → `in progress`.
2. Pomiar bazowy: lista `--lib -- --list`, wyniki `--lib` i `architecture_boundaries`, liczby CRLF.
3. `app/bootstrap.rs`, `pub mod bootstrap;`, nowe `setup` w `lib.rs`, komentarze w testach odtwarzających start.
4. Testy rdzenia; `rustfmt` tylko na `app/bootstrap.rs` i `app/mod.rs`.
5. `--lib`, `architecture_boundaries`, `bun run rust:fmt`, `rust:clippy`, `test:backend`, `git diff --check`.
6. Próby mutacyjne.
7. Dokumentacja (lista w [Pliki](#pliki)).
8. Desktop E2E, jeden spec naraz, i `bun run tauri:dev` na prywatnym Xvfb.
9. `/review-changes HEAD --save` na diffie etapu.

## Pliki

- Nowe:
  - `src-tauri/src/app/bootstrap.rs`
  - ten plik
- Zmienione:
  - `src-tauri/src/lib.rs` (closure `setup`, importy, usunięte `resolve_ffmpeg_path` i helpery)
  - `src-tauri/src/app/mod.rs` (jedna linia)
  - `src-tauri/src/services/backup_service.rs`, `src-tauri/tests/backend_integration.rs` (tylko komentarze)
  - `docs/architecture/system-overview.md`
  - `docs/subsystems/{data-safety-and-portability,thumbnails}.md`
  - `docs/development/{setup-and-build,testing}.md`
  - `CLAUDE.md` („Layout”)
  - `.claude/rules/{backend-core,data-safety,thumbnails}.md`
  - `README.md` i `NOTES.md` planu
- Bez zmian:
  - `generate_handler!`, rejestracja `thumb://`, strażnik profilu, pluginy;
  - `architecture_boundaries.rs` (reguły, allowlisty, `COMMAND_SIGNATURES`, `REGISTERED_COMMANDS`, `dto_snapshot`, eventy);
  - `Cargo.toml`, `Cargo.lock`, frontend, `commands/*`, `usecases/*`, `adapter/*`, `video/*`;
  - ADR 0034 (Proposed; Context opisuje stan na `397181f`).

## Testy

- Nowe (`app::bootstrap::tests`, `tempfile`, bez Tauri):
  - `open_storage_rolls_back_an_interrupted_restore_before_opening_the_database`;
  - `open_storage_sweeps_abandoned_backup_temporaries`;
  - `open_storage_recovers_a_pending_file_operation_and_takes_snapshots`;
  - `open_storage_fails_on_a_corrupt_database_and_leaves_it_in_place`;
  - `open_storage_refuses_a_database_with_a_newer_schema` (dodany po próbie M5: uszkodzony plik odpada już w `open_connection`, więc sam nie pilnuje `?` przy `init_schema`);
  - `start_workers_builds_app_state_for_the_opened_profile`;
  - `resolve_ffmpeg_path_prefers_an_executable_in_the_resource_dir`.
- Komendy:
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib` (i `-- --list`);
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`;
  - `bun run rust:fmt`, `bun run rust:clippy`, `bun run test:backend`, `git diff --check`.
- Desktop E2E (jeden spec naraz): `app.smoke`, `lightbox-recovery` (`MEDIATAGGER_LIGHTBOX_RECOVERY=1`), `startup-scan`, `thumbnail-fade` (`MEDIATAGGER_THUMBNAIL_FADE=1`, okno 503); `bun run tauri:dev` z tymczasowym profilem dev.

## Kryteria akceptacji

- `lib.rs` zawiera tylko składanie; blok `generate_handler!`, rejestracja `thumb://` i strażnik bez zmian w diffie.
- `architecture_boundaries` zielony, plik testu bez zmian.
- `--lib`: passed bez spadku, zbiór nazw sprzed etapu zawarty w nowym.
- Liczby CRLF bez zmian.
- Kolejność startu w `.claude/rules/backend-core.md` i `system-overview.md` zgodna z kodem.

## Ryzyka

- **Kolejność startu.** Przenoszona dosłownie; testy 1–4 pilnują kolejności odzysk restore → sprzątanie → baza → odzysk operacji plikowych → snapshoty, na ile jest obserwowalna.
- **Okno 503.** `manage(AppState)` zostaje ostatnim krokiem `setup`; stronę adaptera łapie tylko E2E.
- **Semantyka porażek.** Ten sam typ błędu i te same `?`.
- **Mieszane końce linii** (patrz etapy 04–06).

## Status

done (2026-11-14). Commit czeka na akceptację użytkownika.

## Notatki agentów

### 2026-11-14 — agent etapu 07
- Zrobione:
  - `src-tauri/src/app/bootstrap.rs` (rdzeń): `open_storage`, `resolve_ffmpeg_path` z helperami i `start_workers` przeniesione dosłownie z `lib.rs` (te same `?`, teksty błędów, komentarze; jedyne nowe linie to dwa klony ścieżek z `&Storage` i składanie wyniku); `BootstrapError = Box<dyn std::error::Error>`; pole `connection` z `#[cfg_attr(not(test), allow(dead_code))]` (tylko trzymane otwarte);
  - `lib.rs`: `setup` = strażnik → `app_data_dir` → `InstanceLock` → `open_storage` → `manage` snapshotów → `resource_dir` → `resolve_ffmpeg_path` → faza wideo (bez zmian) → `manage(start_workers(&storage, …)?)` jako ostatni krok; `storage` upuszczane przy powrocie z `setup`, jak wcześniej `conn`; builder, pluginy, `thumb://`, strażnik i `generate_handler!` nietknięte w diffie; usunięte nieużywane importy;
  - `app/mod.rs`: `pub mod bootstrap;`; komentarze testów odtwarzających start (`backup_service.rs`, `backend_integration.rs`) wskazują `app::bootstrap::open_storage`;
  - dokumentacja: `system-overview.md` (entry points, kroki 5–7 startu, wiersze `lib.rs` i `app/bootstrap.rs`, `StartupScanState`/`StartupPopularTags`, known limitations, checklist, verification map), `thumbnails.md`, `data-safety-and-portability.md`, `setup-and-build.md`, `testing.md`, `CLAUDE.md`, reguły `backend-core` (pełna kolejność startu z podziałem na fazy), `data-safety` i `thumbnails` (ścieżka i punkt o starcie); `batches.json` bez zmian (B12 „Bootstrap, runtime…” ma glob `src-tauri/src/app/*.rs`); README (status, notatki „Z etapu 07” dla 08 i 09), NOTES.
- Odkryte:
  - `resolve_ffmpeg_path` jest wolny od Tauri; `resource_dir` zostaje w adapterze, żeby jego porażka dalej następowała po odzysku i otwarciu bazy (decyzja użytkownika: osobny krok rdzenia, nie `open_storage`);
  - uszkodzony `media.db` odpada już na `PRAGMA journal_mode` w `open_connection`, więc test uszkodzonej bazy nie pilnuje `init_schema`; dodany test bazy z nowszym `user_version` (M5);
  - `bootstrap.rs` nie potrzebuje niczego z adaptera ani z `usecases/`; reguła granicy tego drugiego nie wymusza (M11).
- Zostało: commit po akceptacji; luki z prób (M3b, M6, M7, M8, M11) opisane w notatce „Z etapu 07” dla 09.
- Testy:
  - `--lib`: przed 364 passed, 2 ignored (366 na liście); po 371 passed, 2 ignored (373); zbiór nazw sprzed etapu zawarty w nowym, nowe tylko `app::bootstrap::tests::*` (7);
  - `--test architecture_boundaries`: przed 25 passed; po 25 passed, plik testu bez zmian;
  - `bun run rust:fmt` → pass; `bun run rust:clippy` → pass; `bun run test:backend` → pass (lib 371, `architecture_boundaries` 25, `backend_e2e` 1, `backend_integration` 22 + 1 ignored, `backend_runtime` 6 + 1 ignored, `tag_vocabulary` 6, `thumbnail_bulk` 2, `thumbnail_clear` 2; ignorowane diagnostyki bez zmian); `git diff --check` → pass; `python3 -I -m unittest` (skrypty audytu) → pass (31);
  - liczby CRLF bez zmian (`thumb_service.rs` 423, `thumb_scheduler.rs` 459, `thumbs.rs` 393, `indexer.rs` 232, `main.rs` 2; edytowane i nowe pliki 0); `rustfmt` tylko na `app/bootstrap.rs` i `app/mod.rs`;
  - próby mutacyjne (skrypt w trybie binarnym, każda cofnięta z kontrolą `md5`; pliki CRLF nietykane):
    - M1. `recover_interrupted_restore` po `init_schema` → `open_storage_rolls_back_an_interrupted_restore_before_opening_the_database` czerwony (snapshot rootów z częściowej nowej generacji);
    - M2. bez `remove_abandoned_backup_temporaries` → `open_storage_sweeps_abandoned_backup_temporaries` czerwony;
    - M3a. `recover_pending_file_operations` przed `init_schema` → `open_storage_sweeps_…` i `start_workers_builds_…` czerwone (świeży profil bez tabel); M3b. odzysk po snapshotach → wszystko zielone (**bez strażnika**; snapshoty nie zależą od journala, kolejność pilnowana tylko kodem i dokumentacją);
    - M4. bez `synchronous=FULL` → `open_storage_recovers_a_pending_file_operation_and_takes_snapshots` czerwony;
    - M5. `let _ = db::init_schema(&conn);` → najpierw zielone (luka), po dodaniu testu `open_storage_refuses_a_database_with_a_newer_schema` czerwony;
    - M6. kontrola `worker_spawn_failures` wyłączona → zielone (**bez strażnika**; porażki spawnu wątku nie da się wywołać w teście bez portu);
    - M7. `manage(AppState)` przed fazą wideo w `lib.rs` → cały `cargo test` zielony: **tylko E2E** (i to pośrednio: wcześniejsze `manage` skraca okno 503, nie psuje funkcji); `serve_answers_503_until_app_state_exists` pilnuje strony rdzenia;
    - M8. `resource_dir` + `resolve_ffmpeg_path` + `manage(start_workers(…))` zaraz po `open_storage` (AppState przed snapshotami i wideo) → cały `cargo test` zielony: **tylko E2E/przegląd**;
    - M9. `use tauri::Manager;` w `bootstrap.rs` → reguła 1 (`src/app/bootstrap.rs:17: rule 1: tauri`);
    - M10. `use crate::adapter::startup_scan::StartupScanState;` → reguła 3 (`crate::adapter`);
    - M11. `use crate::usecases;` → zielone (**brak reguły**; notatka „Z etapu 06” to tylko zasada);
  - desktop E2E (prywatny Xvfb, tymczasowe `XDG_*` pod `/tmp/mediatagger-*`, `MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"`, jeden spec naraz, każdy wywołany jawnie z własnymi argumentami w skrypcie bash; profile usunięte po przebiegu):
    - `app.smoke` → 4 passing;
    - `lightbox-recovery` (`MEDIATAGGER_LIGHTBOX_RECOVERY=1`) → 2 passing;
    - `startup-scan` → 1 passing;
    - `thumbnail-fade` (`MEDIATAGGER_THUMBNAIL_FADE=1`) → 2 passing;
  - `bun run tauri:dev` na prywatnym Xvfb z tymczasowym profilem dev, dwa starty: (1) świeży profil → okno „Tagrove”, pusta galeria („No folders attached”), w profilu `instance.lock`, `media.db`, `thumbs/`; (2) po dopisaniu rootu z `auto_scan_on_startup=1` do `scan_roots` → skan startowy, galeria z 6 miniaturami, 6 assetów w bazie; w logach tylko ostrzeżenia środowiska (MESA DRI3, atspi) i SIGTERM z zamknięcia; zamknięcie przez `kill -TERM` grupy procesów uruchomionej własnym `setsid` (nie po nazwie), port 1420 wolny po przebiegu. Pułapka: linia „Running” z `cargo run` ma kody ANSI między słowem a nazwą binarki, więc wzorzec ze spacją i odwrotnym apostrofem jej nie znajduje; trzeba szukać samego `Running` albo pliku `media.db` w profilu.
  - `/review-changes HEAD --save` (soczewki conventions, rust, performance; raport w ignorowanym `raports/`): rust i performance bez uwag; conventions 1 kandydat, weryfikator potwierdził (P3: `data-safety-and-portability.md` opisywał test bootstrapu jako pilnujący odzysku operacji plikowych „przed snapshotami”, czego żaden test nie wymusza, por. M3b). Poprawione w tej zmianie: bullet opisuje tylko stan końcowy i mówi wprost, że kolejność odzysk → pragma FULL → snapshoty wymusza tylko kod; to samo zawężenie w `testing.md` (uwaga weryfikatora o kolejności pragmy, poniżej progu).
