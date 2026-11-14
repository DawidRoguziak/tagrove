# Backend heksagonalny: rdzeń niezależny od Tauri, plan etapowy

Status: **w realizacji** (od 2026-11-14). Etapy 01–02 `done`, pozostałe `todo`. Bieżący stan jest w tabeli [Etapy](#etapy) i w [NOTES.md](NOTES.md). Odnośniki `plik:linia` dotyczą commita `397181f` i mogą się przesunąć; przed edycją agent sprawdza je przez `rg`.

To jest **szkic etapów**, nie pełny plan wykonawczy. Szczegółowy plik `NN-nazwa.md` (format jak w [grid-performance](../grid-performance/README.md)) powstaje dopiero na starcie danego etapu, po ponownym sprawdzeniu kodu.

## Cel

1. Rdzeń backendu (dane, zapytania, mutacje, skan, miniatury, backup, CSV, blokady, bootstrap) nie zależy od `tauri`, `tauri_plugin_*`, `gtk`, `glib`, `webkit2gtk` ani `libmpv2`.
2. Tauri jest adapterem: komendy IPC, eventy, protokół `thumb://`, okno, natywne wideo i składanie aplikacji w `lib.rs`.
3. Docelowo rdzeń jest osobnym crate'em `media_tagger_core` w workspace, więc granicy pilnuje kompilator.
4. Zachowanie i kontrakt wire (nazwy komend, klucze argumentów, pola odpowiedzi, eventy, teksty błędów) nie zmieniają się w żadnym etapie.

## Werdykty z analizy

Analiza 2026-11-13: trzech agentów Explore (Sonnet: sprzężenia z Tauri, graf modułów, testy/build/dokumentacja) i agent Plan (Opus: etapy).

- **Tauri, GTK i libmpv występują w 14 plikach** (poprawione w etapie 01; pierwotnie pominięto `services/video_events.rs`).
  - Adapter z natury: `lib.rs`, `commands/*` (6 plików), `video_surface.rs`.
  - Przecieki do niższych warstw:
    - `app/state.rs`: `StartupScanState` na kanałach `tauri::async_runtime` (`:46,50,82,87`).
    - `services/progress.rs`: jedno `impl ProgressSink for tauri::AppHandle` (`:7-12`).
    - `services/thumb_protocol.rs`: `AppHandle`, `UriSchemeResponder` trzymany głęboko w `Publisher`, `try_state::<AppState>`, `spawn_blocking`.
    - `services/video_player_service.rs`: `tauri::ipc::Channel<VideoEvent>` w `ActiveSession` i `WorkerCommand::Open`.
    - `services/video_events.rs`: `libmpv2`, `libmpv2_sys` (`:10-11`).
    - `services/thumb_service.rs`: tylko w teście (`:3167`, `tauri::http::header`).
- **Reszta jest już wolna od Tauri:** cały `db.rs` i `db/*`, `indexer.rs`, `thumbs.rs` (ffmpeg/ffprobe), `utils/*`, `models.rs`, `error.rs`, `app/{locks,instance_lock,profile}.rs` i większość serwisów. `AppState` sam w sobie jest czystą strukturą.
- **Rdzeń jest synchroniczny.** rusqlite, wątki std, `mpsc`, brak tokio. `spawn_blocking` jest tylko na krawędzi (komendy, `thumb_protocol`).
- **Warstwa use-case istnieje, ale w złym miejscu.** Prywatne funkcje `*_service` w `commands/*.rs` przyjmują `&AppState` i biorą blokady (`with_scan_and_thumb_lock` itd.). Najgrubsze: `find_duplicate_assets`, `start_asset_query` (walidacja, kolejność żądań), `clear_library_data` (bierze konkretny `AppHandle`), `run_startup_scan`, `open_video`, `control_video`.
- **Jedyny port to `ProgressSink`.** `scan_service` i `thumb_service` już przyjmują `&impl ProgressSink`.
- **Cykle wewnątrz rdzenia:** `app::state` ↔ `services` (db_pool, thumb_scheduler kontra backup, scan, thumb_service, thumb_protocol), `db_pool` ↔ `asset_query_service`. Warstwa niższa zależy od adaptera w jednym miejscu: `video_surface.rs` importuje `commands::video::{VideoBounds, VideoControlLabels}`. `db` nie importuje `services` ani `app`.
- **Testy nie używają Tauri.** Cele w `src-tauri/tests` importują `media_tagger::{db, models, services, app, error}`. Cztery z nich (`backend_runtime`, `grid_baseline`, `thumbnail_bulk`, `thumbnail_clear`) budują `AppState` literałem struktury.
- **Build:** brak workspace. Ścieżki `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/target*` są wpisane na sztywno w `package.json`, `e2e/build-e2e.js`, `Dockerfile.linux`, `Dockerfile.native`, `packaging/**` i `scripts/privacy-check.py`. Workspace z korzeniem w `src-tauri/Cargo.toml` zostawia te ścieżki bez zmian. `packaging/flatpak/generate.py` pomija pakiety bez `source`, więc lokalny crate nie psuje vendoringu. Dockerfile kopiują tylko manifest i stub `lib.rs`, więc trzeba je poprawić.

## Stan docelowy (szkic)

```
src-tauri/                          korzeń workspace + pakiet adaptera `media_tagger` (bin bez zmiany nazwy)
  Cargo.toml                        [workspace] members = [".", "crates/media_tagger_core"]
  Cargo.lock, target/, target-e2e/  ścieżki bez zmian
  tests/                            cele integracyjne zostają tutaj, importy z media_tagger_core
  src/                              ADAPTER: tauri, pluginy, gtk, webkit2gtk, libmpv2*, async-channel
    lib.rs                          Builder, pluginy, strażnik identyfikatora, generate_handler!, manage()
    commands/*.rs                   cienkie: argumenty → spawn_blocking → use case → map_err(String) / ipc::Response
    adapter/events.rs               impl ProgressSink dla AppHandle ("process-progress")
    adapter/thumb_scheme.rs         UriSchemeResponder, try_state, spawn_blocking, 503 przed AppState
    adapter/startup_scan.rs         StartupScanState (kanały async) + orkiestracja run_startup_scan
    app/profile.rs                  allowlista identyfikatorów (sprawa konfiguracji Tauri)
    video/                          player libmpv + video_events + powierzchnia GTK + typy Bounds/Labels
  crates/media_tagger_core/src/     RDZEŃ: bez tauri/gtk/webkit/libmpv; rusqlite, serde, image, zip, csv dozwolone
    models.rs, error.rs, utils/     typy domenowe i DTO (serde zostaje)
    db.rs, db/*                     SQLite używany bezpośrednio, bez traitów repozytoriów
    indexer.rs, thumbs.rs           fs + ffmpeg/ffprobe używane bezpośrednio
    services/*                      obecne serwisy (query, mutation, backup, csv, scan, thumb_*, selection, video_source)
    services/thumb_protocol.rs      czysta część: parse_request, allowed_origin, ThumbReply{status, headers, body}
    usecases/{assets,scan,thumbs,library}.rs   dawne prywatne *_service z commands/*
    app/{state,locks,instance_lock,bootstrap}.rs
    ports.rs                        ProgressSink, ThumbResponder
```

## Decyzje

| # | Pytanie | Decyzja | Uzasadnienie |
| --- | --- | --- | --- |
| D1 | Gdzie żyje wideo (libmpv) | **Cały stos w adapterze** (`src/video/`); rdzeń ma tylko `video_source_service` (decyzja użytkownika) | libmpv dzieli uchwyt z kontekstem renderowania GTK; to runtime GUI. Bez portu `VideoEventSink`. |
| D2 | Osobny crate czy granica w crate'cie | **Crate jako cel** (etap 08) (decyzja użytkownika) | Etapy 01–07 mają wartość same w sobie; 08 daje egzekucję przez kompilator. Wpis w `Cargo.lock` za zgodą przy etapie 08. |
| D3 | Nazwa crate'a rdzenia | `media_tagger_core` | Polityka nazw wewnętrznych (`docs/architecture/system-overview.md`, „naming”). |
| D4 | Gdzie testy integracyjne po wydzieleniu | Zostają w `src-tauri/tests`, importy z `media_tagger_core` | Nazwy celów w `package.json`, CLAUDE.md i dokumentacji zostają ważne. |
| — | rusqlite w rdzeniu | Tak, bez traitów repozytoriów | Jeden backend; inwarianty (bump rewizji w tej samej transakcji, triggery sort-key, permity `DatabaseRuntime`) są związane z transakcjami SQL. Testy już działają na in-memory SQLite. |
| — | serde w rdzeniu | Tak | `models.rs` to jednocześnie domena i format wire. Podział zdublowałby 437 linii i groził dryfem. Osobne DTO tylko tam, gdzie kształty faktycznie się rozjadą. |
| — | `http` w rdzeniu | Nie | Byłaby to nowa bezpośrednia zależność. Rdzeń zwraca `ThumbReply`, adapter buduje `tauri::http::Response`. |

## Czego nie robimy

- Traity repozytoriów / unit of work dla bazy i przepisywanie `db/*`.
- Porty dla fs, zegara, `rand`, env (`MEDIATAGGER_PERF`) i `Command::new` ffmpeg. Nie mają drugiej implementacji.
- Zmiana nazwy `services` na „application” albo podział na crate'y domain/app/infra. Jeden crate rdzenia wystarczy.
- Rozbijanie cykli wewnątrz rdzenia (state ↔ services, db_pool ↔ asset_query), o ile etap tego nie wymaga.
- Typowane błędy IPC, zmiany atrybutów serde, zmiany tekstów błędów. To zmiany wire, nie refaktor.
- async/tokio w rdzeniu. Rdzeń zostaje synchroniczny, `spawn_blocking` tylko w adapterze.
- Przenoszenie testów do crate'a rdzenia podczas wydzielania.
- Worktree (zakazane w `CLAUDE.md`).

## Egzekucja granicy

- **Etapy 01–07:** test `src-tauri/tests/architecture_boundaries.rs` (tylko std), skanuje `CARGO_MANIFEST_DIR/src`.
  - Reguła 1: tokeny `tauri`, `tauri_plugin_*`, `AppHandle`, `gtk`, `gdk`, `glib`, `webkit2gtk`, `libmpv2`, `libmpv2_sys`, `async_channel`, `libloading` tylko w stałym zbiorze adaptera (`lib.rs`, `main.rs`, `commands/**`, `adapter/**`, `video/**`, `app/profile.rs`) i na przejściowej allowliście, która może tylko maleć.
  - Reguła 2: `db` nie importuje `services`, `app`, `commands`, `usecases` (dziś prawda; test to zamraża).
  - Reguła 3: żaden plik rdzenia nie importuje `commands`, `adapter`, `video` ani `video_surface`. Przejściowa allowlista: `video_surface.rs` (do etapu 04).
  - Reguła 4 (od etapu 06): `commands/**` nie zawiera `db::`, `with_*lock` ani `Connection`.
- **Od etapu 08:** granicę pilnuje crate.
  - `cargo tree -p media_tagger_core --locked -e normal` nie zawiera `tauri`, `gtk`, `webkit2gtk`, `libmpv`.
  - `cargo build -p media_tagger_core --locked` przechodzi.
  - Test grep zostaje dla reguł 2 i 4.

## Etapy

| Etap | Nazwa | Fala | Zależy od | Status |
| --- | --- | --- | --- | --- |
| 01 | Strażnik granicy, snapshot wire, ADR 0034 Proposed | A | — | done 2026-11-14 ([plik etapu](01-boundary-guard.md)) |
| 02 | Postęp i testy bez Tauri | B | 01 | done 2026-11-14 ([plik etapu](02-progress-sink.md)) |
| 03 | Startup scan do adaptera | B | 01 | todo |
| 04 | Izolacja wideo (D1) | B | 01 | todo |
| 05 | Podział `thumb://` | B | 01 | todo |
| 06 | Warstwa use-case, cienkie komendy | C | 02, 03 | todo |
| 07 | Bootstrap w rdzeniu | C | 03, 04, 05 | todo |
| 08 | Wydzielenie crate'a `media_tagger_core` | D | 01–07 | todo |
| 09 | Dokumentacja, reguły, skille, ADR Accepted, pełny E2E | E | 08 | todo |

Statusy: `todo`, `in progress`, `blocked`, `done`, plus data. Zmianę statusu zapisuje orkiestrator w tej tabeli i w [NOTES.md](NOTES.md).

### Fale i równoległość

- **Fala A:** 01 sam.
- **Fala B:** 02 ∥ 03 ∥ 04 ∥ 05, na rozłącznych plikach. Importy w `src-tauri/src/lib.rs` zmieniają kolejno: 03 → 05 → 04.
- **Fala C:** 06 ∥ 07. Etap 06 nie dotyka `lib.rs`. Po utworzeniu `usecases/mod.rs` etap 06 można dzielić per plik komend (assets ∥ thumbs ∥ import_export ∥ scan).
- **Fala D:** 08 sam.
- **Fala E:** 09.

Każdy etap jest osobnym, działającym commitem. Plan można zatrzymać po etapie 07 bez straty wartości.

### 01 Strażnik granicy, snapshot wire, ADR

- **Cel:** zamrozić stan i mierzyć postęp.
- **Zakres:**
  - `src-tauri/tests/architecture_boundaries.rs` z regułami 1–3 (reguła 4 jako `#[ignore]`).
  - Snapshot kontraktu wire w tym samym celu (jeden cel zamiast dwóch: mniej linkowania, zasada skilla `tauri-rust-tests`):
    - komendy z `generate_handler!` porównane z `docs/architecture/ipc-contract.md`;
    - sygnatury wire;
    - JSON DTO;
    - nazwy eventów.
  - [ADR 0034](../../adr/0034-tauri-independent-backend-core.md) w stanie `Proposed`: układ docelowy, reguły zależności, D1–D4.
- **Wyjście:**
  - nowe testy zielone;
  - allowlista reguły 1 ma 7 wpisów (plus 7 plików w stałym zbiorze adaptera);
  - allowlista reguły 3 ma 1 wpis;
  - `bun run test:backend` zielony.
- **Ryzyko:** zbyt szeroki grep. Dopasowuj tokeny i ścieżki `use`, nie podciągi w komentarzach.

### 02 Postęp i testy bez Tauri

- **Zakres:**
  - `impl ProgressSink for AppHandle` z `services/progress.rs` → `src/adapter/events.rs`.
  - `clear_library_data_service` (`commands/import_export.rs:69`) przyjmuje `&impl ProgressSink`.
  - Asercja `tauri::http` z testu w `thumb_service.rs:3167` → testy `thumb_protocol`.
- **Wyjście:** `progress.rs` i `thumb_service.rs` znikają z allowlisty.
- **E2E:** `settings-maintenance`.
- **Ryzyko:** minimalne.

### 03 Startup scan do adaptera

- **Zakres:** `StartupScanState` i `StartupScanRequest` (`app/state.rs:46-90`) oraz `run_startup_scan` (`commands/scan.rs:~180-250`) → `src/adapter/startup_scan.rs`. Rdzeń zostawia snapshot włączonych rootów i synchroniczne `scan_roots`.
- **Wyjście:** `app/state.rs` znika z allowlisty, testy startup scanu przeniesione i zielone.
- **E2E:** `startup-scan`, `startup-scan-remount`.
- **Ryzyko:** semantyka współdzielonego czekania i odtwarzania wyniku. Przenoś kod bez zmian logiki.

### 04 Izolacja wideo (D1: cały stos w adapterze)

- **Zakres:**
  - `services/video_player_service.rs`, `services/video_events.rs`, `video_surface.rs` → `src/video/`.
  - `VideoBounds`, `VideoControlLabels` z `commands/video.rs` → `src/video/types.rs`.
  - `services/video_source_service.rs` zostaje w rdzeniu.
- **Wyjście:** `rg 'commands::' src-tauri/src/video` puste, testy playera zielone, pliki wideo w stałym zbiorze adaptera zamiast na allowliście.
- **E2E:** `native-video-escape`, `video-speed` oraz ręcznie przez `/verify-mediatagger`.
- **Ryzyko:** model wątków GTK/libmpv ([ADR 0012](../../adr/0012-non-blocking-native-video-render.md)). Tylko przeniesienie, bez zmian zachowania.
- Z etapu 01: etap usuwa też wpis `video_surface.rs` z `ADAPTER_IMPORT_ALLOWLIST` (reguła 3) i trzy wpisy wideo z `FRAMEWORK_ALLOWLIST`. Importy `commands::video::*` w `dto_snapshot` zmieniają ścieżkę, a JSON zostaje bez zmian.

### 05 Podział `thumb://`

- **Zakres:**
  - Rdzeń: `parse_request`, `allowed_origin`, odpowiedzi jako `ThumbReply`, logika `Publisher` generyczna po porcie `ThumbResponder` (albo `Box<dyn FnOnce(ThumbReply)>`).
  - `src/adapter/thumb_scheme.rs`: `UriSchemeResponder`, `try_state`, `spawn_blocking`, 503 zanim istnieje `AppState`.
- **Wyjście:** plik rdzenia znika z allowlisty; testy protokołu (status, CORS, `Cache-Control`, `p=1`, `c=1`) zielone bez zmian.
- **E2E:** `thumbnail-fade`, `gallery-browsing`.
- **Ryzyko:** utrata kolejności LIFO, anulowania prefetchu albo 503 przed stanem ([ADR 0006](../../adr/0006-thumb-uri-scheme.md), [ADR 0021](../../adr/0021-visible-tiles-drop-a-pending-prefetch.md)).

### 06 Warstwa use-case, cienkie komendy

- **Zakres:**
  - Prywatne `*_service` z `commands/{assets,scan,thumbs,import_export}.rs` → `src/usecases/*.rs`, łącznie z grubymi (`find_duplicate_assets`, walidacja `start_asset_query`, `clear_library_data`).
  - `get_asset_query_id_range`: use case zwraca bajty, adapter owija w `ipc::Response`.
  - Helpery blokad wołane tylko w use case'ach.
  - `open_video` i `control_video` zostają w adapterze (D1).
- **Wyjście:** reguła 4 zielona, handlery komend mają ~15 linii, `test:backend` zielony.
- **E2E:** `app.workflows`, `gallery-selection`, `tag-editor`, `settings-maintenance`.
- **Ryzyko:** przypadkowa zmiana tekstu błędu albo blokada trzymana przez `.await`. Przenoś dosłownie, diff przez `/review-changes`.
- Z etapu 01: reguła 4 to test `commands_do_not_touch_db_or_locks` z `#[ignore]`. Stan wyjściowy (`-- --ignored`): `scan.rs` 24, `assets.rs` 21, `thumbs.rs` 6, `import_export.rs` 3 trafienia. Etap usuwa `#[ignore]`. `COMMAND_SIGNATURES` musi zostać bez zmian.

### 07 Bootstrap w rdzeniu

- **Zakres:** closure `setup` w `lib.rs` dzieli się na fazy.
  - `core::app::bootstrap::open_storage(paths)`: odzysk restore → sprzątanie tymczasowych plików backupu → `thumbs/` → DB + `init_schema` → odzysk oczekujących operacji plikowych → snapshoty → `resolve_ffmpeg_path`.
  - Adapter: `LC_NUMERIC`, player i powierzchnia wideo.
  - `start_workers()`: scheduler miniatur i `AppState`.
  - Strażnik identyfikatora, `app_data_dir`, `resource_dir` i `InstanceLock` przed DB zostają w adapterze.
- **Wyjście:** `lib.rs` zawiera tylko składanie; nowy test rdzenia uruchamia bootstrap na tempdir bez Tauri; kolejność startu w `.claude/rules/backend-core.md` zgadza się z kodem.
- **E2E:** `app.smoke`, `lightbox-recovery` oraz uruchomienie `bun run tauri:dev`.
- **Ryzyko:** kolejność startu to twardy inwariant; okno 503 dla `thumb://` musi zostać.

### 08 Wydzielenie crate'a `media_tagger_core`

- **Zakres:**
  - `[workspace]` w `src-tauri/Cargo.toml`, `default-members` obejmuje oba pakiety, żeby `--manifest-path` w skryptach nadal obejmował rdzeń.
  - `git mv` plików rdzenia do `src-tauri/crates/media_tagger_core/`, podział zależności.
  - Testy w `src-tauri/tests` z podmienionymi importami.
  - `Dockerfile.linux`, `Dockerfile.native` (manifest członka + stub), `packaging/linux/prepare-source.py`, `source-info.sh`, `notices.py`; przegenerowanie `packaging/flatpak/generate.py`.
  - `Cargo.lock` dostaje wpis lokalnego pakietu: **jawna zmiana lockfile'a za zgodą użytkownika**.
- **Wyjście:** kontrole `cargo tree` i `cargo build -p` z sekcji [Egzekucja granicy](#egzekucja-granicy); `bun run quality` i `bun run test:backend` zielone; `bun run tauri:build:e2e` + `app.smoke`; próbny build Docker lub Flatpak (tylko orkiestrator).
- **Ryzyko:** `--test X` razem z `default-members` (sprawdzić); cache warstw w Dockerze; tarball źródeł bez `crates/`.
- Z etapu 02: `impl ProgressSink for AppHandle` w `src/adapter/events.rs` po wydzieleniu łamie regułę sierot (E0117: trait z `media_tagger_core`, typ z `tauri`). Potrzebny newtype w adapterze (np. `TauriProgress(AppHandle)`) i podmiana `&app` na niego w wywołaniach z `commands/*` (scan, thumbs, import_export, assets) albo closure przez blanket impl `Fn`. Wire bez zmian.
- Z etapu 01: `source_roots()` w `architecture_boundaries.rs` dopisuje `crates/media_tagger_core/src`. Reguły 2–3 liczą ścieżki względem korzenia, więc rdzeń i adapter trzeba rozróżnić po korzeniu. Importy `dto_snapshot` przechodzą na `media_tagger_core`.

### 09 Dokumentacja, reguły, skille, pełny E2E

- **Zakres:**
  - `docs/architecture/system-overview.md`: diagram warstw, tabela modułów, safe-change checklist, verification map.
  - `docs/architecture/ipc-contract.md`: ścieżki w „Relevant tests”.
  - Globy `paths:` w `.claude/rules/*.md` (backend-core, database, data-safety, ipc-contract, scanning, thumbnails, lightbox-video, testing-e2e) o `src-tauri/crates/media_tagger_core/src/**`.
  - Sekcja „Layout” w `CLAUDE.md`.
  - Skille: `.claude/skills/review-changes/scripts/collect_scope.py` i evals, `.claude/skills/audit/references/batches.json`.
  - ADR 0034 → `Accepted`.
- Każdy wcześniejszy etap aktualizuje swoją dokumentację w tej samej zmianie; ten etap to końcowe przeczesanie.
- **Wyjście:** `rg 'src-tauri/src/(db|services|models|indexer|thumbs)' docs .claude CLAUDE.md` puste poza historycznymi planami; pełny przebieg E2E, jeden spec naraz.

## Zasady dla agentów

- Przed startem przeczytaj ten README, plik swojego etapu, [NOTES.md](NOTES.md), [system-overview.md](../../architecture/system-overview.md), [ipc-contract.md](../../architecture/ipc-contract.md) i reguły `.claude/rules/backend-core.md`, `database.md`, `ipc-contract.md`.
- **Kontrakt wire nie zmienia się:** te same komendy w `generate_handler!`, klucze argumentów, pola odpowiedzi, eventy i teksty błędów. Błędy zostają `String` przez istniejące `map_err`.
- **Przenoś kod dosłownie.** Refaktor logiki to osobna zmiana z osobnym uzasadnieniem.
- **Allowlistę** w `architecture_boundaries.rs` zmienia tylko orkiestrator, po akceptacji etapu.
- **`lib.rs`** edytuje jeden agent naraz, w kolejności z [Fale i równoległość](#fale-i-równoległość).
- **Kolejność startu** i **kolejność blokad** (maintenance gate → `scan_lock` → `thumb_lock`, tylko przez `app/locks.rs`, nigdy przez `.await`) bez zmian.
- **Desktop E2E uruchamia tylko orkiestrator, jeden spec naraz.** Agenci uruchamiają `cargo test --manifest-path src-tauri/Cargo.toml --locked <filtr>` oraz `bun run rust:fmt` i `bun run rust:clippy`.
- **Zależności:** `cargo` zawsze z `--locked`. Nowa zależność albo zmiana `Cargo.lock` tylko za zgodą użytkownika.
- **Nazwy wewnętrzne** (`media_tagger`, identyfikatory, `MEDIATAGGER_*`) zostają.
- **Repozytorium jest publiczne:** w notatkach żadnych prywatnych ścieżek ani danych.
- **Zakaz worktree.** Równolegle pracują tylko agenci na rozłącznych plikach.
- **Commit** dopiero po akceptacji użytkownika, osobno dla każdego etapu.

## Protokół notatek

Plik etapu (`NN-nazwa.md`, tworzony na starcie etapu) kończy się sekcjami `## Status` i `## Notatki agentów`.

- `## Status`: `todo`, `in progress`, `blocked` albo `done`, plus data. Ta sama wartość trafia do tabeli [Etapy](#etapy).
- `## Notatki agentów`: wpisy tylko dopisywane:

  ```
  ### YYYY-MM-DD — <rola, np. agent etapu 03>
  - Zrobione: …
  - Odkryte: … (plik:linia)
  - Zostało: …
  - Testy: `<komenda>` → pass/fail
  ```

- Po każdym wpisie dopisz jedną linię do [NOTES.md](NOTES.md).
- Odkrycie, które zmienia inny etap, zapisz w tamtym etapie (albo w jego sekcji tego README, jeśli plik jeszcze nie istnieje) z prefiksem `Z etapu NN:`.
