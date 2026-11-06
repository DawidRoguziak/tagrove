# Etap 02: kontencja w backendzie (pula, skan, blokady, zapisy miniatur)

Fala A. Warstwa: Rust.

Dokumentacja do przeczytania:
- `docs/subsystems/database.md` (pula, transakcje, rewizje);
- `docs/subsystems/scanning-and-indexing.md`;
- `docs/subsystems/thumbnails.md`;
- `docs/architecture/system-overview.md` (blokady).

## Problemy (potwierdzone w kodzie)

1. **Mała pula.** `src-tauri/src/services/db_pool.rs:10` ma `MAX_CONNECTIONS = 4`. Czekanie na połączenie trwa do 5 s (`db_pool.rs:99`), potem pojawia się błąd „pool busy”.
2. **Skan trzyma 2 połączenia.** `src-tauri/src/commands/scan.rs:23` i `:117` biorą `conn` i trzymają je, a `scan_roots` (`src-tauri/src/services/scan_service.rs:81`) bierze drugie na cały skan. Do tego `start_asset_query` trzyma połączenie przez cały rebuild wektora ID. Na zapis użytkownika zostaje 0–1 połączenie.
3. **Bump rewizji co 512 plików** (`scan_service.rs:401-403`). Każdy batch unieważnia wszystkie sesje, a frontend robi pełny `refresh()` z rebuildem wektora. Przy skanie 100k plików to około 200 rebuildów w trakcie przeglądania.
4. **Zapis favorite lub tagów** ma do 4 prób po 5 s busy timeout (`src-tauri/src/db/mutations.rs:639-660`, `src-tauri/src/db/connections.rs:17`). W najgorszym razie to ~25 s z wyłączonym przyciskiem.
5. **Dwa zapisy na każdą miniaturę.** Najpierw transakcja z aktualizacją ścieżki (`src-tauri/src/services/thumb_service.rs:727-744`), potem osobny `DELETE FROM thumbnail_failures`. Bulk renderer też usuwa failures pojedynczo. Każdy zapis walczy o write lock z batchami skanu.
6. **`thumb_lock` głodzi galerię.**
   - `delete_asset` i `rename_asset_file` biorą wyłączny `thumb_lock` (`src-tauri/src/commands/assets.rs:551, 626`).
   - `ensure_thumbnails` trzyma współdzielony lock przez cały przebieg, nawet ~90 s dla wideo.
   - `RwLock` na Linuksie przepuszcza czekającego writera przed nowymi readerami, więc jeden delete wstrzymuje wszystkie miniatury.
7. **Upgrade transakcji skanu z odczytu do zapisu** (podejrzenie, `scan_service.rs:391`): `BEGIN` deferred plus zapis po odczycie może dać `SQLITE_BUSY_SNAPSHOT`, na który busy timeout nie pomaga.

## Kroki

1. **Połączenia skanu.** Ogranicz zasięg `conn` w `scan_folder` i `rescan_all_roots` do miejsca, gdzie jest potrzebne (blok `{}` albo jawne `drop`). Skan używa jednego połączenia.
2. **Pula.**
   - Podnieś `MAX_CONNECTIONS` do 8.
   - Rozważ rezerwę: 1–2 połączenia dostępne tylko dla krótkich zapisów użytkownika (favorite, tagi, grupa). Na przykład osobny semafor lub osobna mini-pula.
   - Zaktualizuj `database.md`. Dodaj test wyczerpania puli (`docs/subsystems/database.md` notuje jego brak).
3. **Throttling rewizji w skanie.**
   - Najpierw ustal, dlaczego komentarz przy `scan_service.rs:401` wymaga bumpa w każdym batchu („a crash can never leave indexed rows without a revision bump”). Sesje zapytań są w pamięci, więc po crashu nie istnieją.
   - Sprawdź, czy coś trwałego zależy od rewizji: `query_revisions`, cache po stronie frontendu w localStorage, backup.
   - Jeśli nic: bump najwyżej raz na ~1 s (pierwszy batch zawsze od razu), plus obowiązkowo na końcu skanu, przy przerwaniu i przy błędzie. Rozważ też bump przy starcie aplikacji.
   - Jeśli coś zależy: zostaw bump w transakcji, ale wprowadź osobną rewizję „scan progress”, którą frontend odświeża z throttlingiem (wtedy zmiana trafia do etapu 04).
   - Zapisz decyzję w notatkach i w `database.md` lub `scanning-and-indexing.md`.
4. **Zapisy miniatur.**
   - Aktualizacja `thumb_path` i usunięcie z `thumbnail_failures` w jednej transakcji.
   - W bulk rendererze i w `ensure_page_thumbnails_with_sink`: grupowanie wyników (np. co ~50 ms albo 16 sztuk) w jednej transakcji.
   - Zachowaj compare-and-set na `SourceVersion`.
5. **`thumb_lock`.** Współdzielony guard brany na czas pojedynczego zadania (render i publikacja jednego pliku), nie na cały przebieg `ensure_thumbnails`. Czekanie na scheduler bez trzymania locka. Pilnuj kolejności blokad i helperów z `src-tauri/src/app/locks.rs`.
6. **Transakcja batcha skanu.** `BEGIN IMMEDIATE` (`TransactionBehavior::Immediate`) dla batcha zapisu.
7. **Diagnostyka.** Przy `MEDIATAGGER_PERF=1` loguj czas czekania na połączenie z puli i czas transakcji zapisu dla favorite i tagów (bez ścieżek plików).

## Pliki

- `src-tauri/src/services/db_pool.rs`, `scan_service.rs`, `thumb_service.rs`, `thumb_scheduler.rs` (tylko tam, gdzie dotyczy locka lub zapisów)
- `src-tauri/src/commands/scan.rs`, `commands/assets.rs` (blokady delete i rename)
- `src-tauri/src/app/locks.rs` (jeśli potrzebny nowy helper)
- `src-tauri/src/db/thumbnails.rs` lub inne miejsce zapisów miniatur
- `docs/subsystems/database.md`, `scanning-and-indexing.md`, `thumbnails.md`, `docs/architecture/system-overview.md` (blokady)

## Testy

- `src-tauri/tests/backend_runtime.rs`:
  - wyczerpanie puli: N−1 połączeń trzymanych przez symulowany skan; `set_asset_favorite_service` kończy się w < 1 s;
  - skan nie trzyma więcej niż jednego połączenia.
- Testy jednostkowe throttlingu rewizji: pierwszy batch daje bump; batche w oknie 1 s nie dają; koniec i błąd zawsze dają.
- Test: czekający exclusive `thumb_lock` nie blokuje nowego zadania miniatury dłużej niż jedno zadanie.
- Test: jedna transakcja na wynik miniatury (licznik commitów lub brak wpisu w failures po sukcesie).
- Regresja: `cargo test --manifest-path src-tauri/Cargo.toml --locked --test backend_integration` oraz `--test thumbnail_bulk`, `--test thumbnail_clear`.
- `bun run rust:fmt && bun run rust:clippy`.

## Kryteria akceptacji

- Podczas skanu dużego folderu favorite i tag zapisują się w < 500 ms (pomiar w etapie 07).
- Liczba rebuildów wektora ID podczas skanu ograniczona do około 1 na sekundę albo mniej.
- Delete lub rename nie zatrzymuje ładowania miniatur galerii na dłużej niż jedno zadanie.

## Ryzyka

- Throttling może zostawić krótko nieaktualny widok podczas skanu. To akceptowalne, o ile końcowy bump jest gwarantowany.
- Więcej połączeń SQLite oznacza więcej pamięci (cache 64 MiB na połączenie?). Sprawdź `PRAGMA cache_size` per połączenie; ewentualnie zmniejsz dla połączeń zapisu.

## Status

done — 2026-11-07 (commit czeka na akceptację użytkownika). Decyzje: [ADR 0002](../../adr/0002-backend-write-priority-and-scan-revisions.md).

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->

### 2026-11-06 — orkiestrator (etap 07, krok 0)
- Z etapu 07: pomiar bazowy `src-tauri/tests/grid_baseline.rs` (release, baza na dysku, biblioteka 100k, 10 skanów `scan_folder` po 20k nowych plików, zapis użytkownika co 50 ms, odczyt strony co 100 ms).
  - Favorite bez skanu: p50 0,02 ms. Podczas skanu (trzy biegi): p50 4–53 ms, p95 130–429 ms, max 329–529 ms.
  - Tagi bez skanu: p50 0,12 ms. Podczas skanu: p50 18–34 ms, p95 179–430 ms, max 531–729 ms.
  - Nieudane zapisy: 0 z 68, 0 z 74 i 0 z 82. Czekanie na połączenie z puli: max 0,6 ms. Wyczerpania puli i scenariusza „~25 s” (punkt 4) przy tym obciążeniu nie zaobserwowano.
  - Bumpy rewizji: 43–49 na skan 20k plików (punkt 3 potwierdzony). Przy odczycie strony co 100 ms prawie każda strona jest `stale`: 44–49 rebuildów na 54–59 odczytów. Rebuild przy 100k pod obciążeniem skanem: p50 80 ms, p95 126 ms (bez skanu 30 ms).
  - W E2E (debug) żaden zapis nie zwrócił błędu; `set_asset_tags` p50 35 ms, max 230 ms.
- Wniosek: dla objawu „like nie działa” backend nie jest przyczyną (patrz etap 01). Ten etap dotyczy opóźnień (max ~0,3–0,7 s), lawiny rebuildów i miniatur.

### 2026-11-06 — agent etapu 01
- Z etapu 01: po wprowadzeniu kolejki per asset wszystkie zapisy dochodzą do backendu, więc jego opóźnienie widać teraz wprost jako czas pending. W `grid-baseline.e2e.js` (debug, skan 20k co ~1,4 s) zapisy podczas skanu: `set_asset_tags` max 3032 ms i 1431 ms, `set_asset_favorite` 929–1532 ms w 3 z 20 prób; pozostałe poniżej ~330 ms. Serce za takim tagiem czeka do ~3 s. To jest cel tego etapu.

### 2026-11-07 — agent etapu 02
- Zrobione (szczegóły i odrzucone warianty: ADR 0002):
  - Krok 1: skan trzyma jedno połączenie. `scan_folder_service` i `rescan_all_roots_service` (`src-tauri/src/commands/scan.rs`) zwalniają połączenie rejestracji przed `scan_roots`.
  - Krok 2: pula 8 połączeń, zwykłe checkouty najwyżej 6, rezerwa 2 dla `user_write_connection()` (favorite, tagi, grupy, także bulk).
    - Bilet zapisu użytkownika plus `yield_to_user_writes()` (≤ 250 ms na transakcję): skan i bulk miniatur ustępują przed każdą transakcją.
    - Busy handler zapisu użytkownika odpytuje co 1 ms w budżecie 5 s (`db::poll_busy_lock_quickly`).
  - Krok 3: `ScanRevisionThrottle`: pierwszy batch od razu, potem ≤ 1 bump/s. Prune z usunięciami bumpuje sam. Końcowy bump bezwarunkowy. Bump na błędzie lub panice z `Drop` dla `ScanWriter`.
  - Kroki 4 i 6: batch, touch i prune skanu w `BEGIN IMMEDIATE`. Wyniki miniatur w `commit_thumbnail_results` (CAS plus marker failure w jednej transakcji). Strumień grupuje gotowe wyniki (≤ 64). Bulk commituje po ≤ 256 z yield. Istniejąca miniatura nie wymaga zapisu.
  - Krok 5: `with_thumb_operation` plus `thumb_read_section`. Guard tylko na krótkie sekcje, nigdy w czasie czekania na scheduler. Bulk bierze epokę clear per stronę. `with_thumb_read_lock` usunięty.
  - Krok 7: `[perf] name=set_asset_favorite|set_asset_tags pool_wait_ms write_ms`. Linia `[perf] name=scan` ma pola `revision_bumps`, `user_write_yields` i `yield_ms`.
  - Dokumentacja: `database.md`, `scanning-and-indexing.md`, `thumbnails.md`, `system-overview.md` oraz jedno zdanie o `ready` w `ipc-contract.md` (za zgodą użytkownika). ADR 0002 jest w indeksie.
- Odkryte:
  - Główna przyczyna opóźnień to głodzenie zapisu. Bundlowany SQLite (`HAVE_USLEEP`) śpi w busy handlerze do 100 ms na próbę, a batche skanu idą prawie bez przerw.
    - W lokalnych wynikach E2E każda wolna operacja przypadała na fazę `scan`; `remove_scan_root` trwał 184 ms.
    - Test `user_write_is_not_starved_by_back_to_back_background_transactions` bez priorytetu kończy się błędem po ~20 s (4 × 5 s busy timeout), a z priorytetem przechodzi.
  - Punkt 7 problemów się nie potwierdził. Batch skanu zaczyna się od `INSERT … ON CONFLICT` (`src-tauri/src/db/scan_membership.rs:26`), więc nie ma upgrade'u odczytu do zapisu ani `SQLITE_BUSY_SNAPSHOT`. `IMMEDIATE` wprowadzone dla jawności.
  - Nic trwałego nie zależy od bumpa w każdym batchu.
    - Sesje zapytań i migawka duplikatów żyją w pamięci, a markery odbudowuje start (`db/query_revisions.rs:6`).
    - Rozwiązywanie duplikatów bierze `scan_lock`, więc widzi bump końcowy.
    - Etap 04 nie potrzebuje osobnej rewizji „scan progress”. Bump przy starcie nie jest potrzebny.
  - Istniejąca miniatura robiła 2 zapisy (no-op UPDATE też bierze blokadę zapisu), czyli do 256 zapisów na jedno `ensure_thumbnails`.
  - `src-tauri/src/services/thumb_service.rs` ma w repo mieszane końce linii (CRLF i LF). `rustfmt` w trybie zapisu normalizuje cały plik, a `--check` tego nie zgłasza. Niezmienione linie zachowały CRLF, żeby diff pokazywał tylko faktyczne zmiany.
  - Metryka `revision_bumps` w `grid_baseline.rs` liczy też bumpy samych zapisów użytkownika (favorite bumpuje zawsze), więc po zmianie skan dokłada do niej ~2 bumpy.
- Pomiar Rust (`TMPDIR=/var/tmp cargo test --manifest-path src-tauri/Cargo.toml --locked --release --test grid_baseline -- --ignored --nocapture --test-threads=1`). Host bez obciążenia przez drugą sesję (`pgrep` puste, load ~0,6–1,3). Przed: HEAD `9d8a721`, drzewo czyste. Po: drzewo z tym etapem.

  | Metryka (10 skanów po 20k, 100k wierszy) | Przed | Po |
  | --- | --- | --- |
  | favorite podczas skanu p50 / p95 / max (ms) | 34 / 129 / 179 | 4,4 / 24 / 35 |
  | tag podczas skanu p50 / p95 / max (ms) | 34 / 529 / 639 | 4,4 / 19 / 31 |
  | rebuildy na skan (strona co 100 ms) | 4–5 (49 na 60 odczytów) | 1 (10 na 70 odczytów) |
  | `revision_bumps` na skan (z zapisami użytkownika) | 45–53 | 8–17 |
  | czas skanu (ms) | 811–1178 | 665–801 |
  | operacje użytkownika: wykonane / nieudane / > 1 s | 84 / 0 / 0 | 138 / 0 / 0 |
  | czekanie na pulę max (ms) | 0,32 | 0,32 |
- Testy:
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked` → pass (204 unit plus wszystkie targety integracyjne).
  - Nowe testy:
    - `db_pool.rs`: rezerwa, przywrócenie busy timeout, ograniczony yield;
    - `backend_runtime.rs`: wyczerpane zwykłe checkouty, transakcje w tle jedna za drugą;
    - `scan_service.rs`: throttle, bump z `Drop`, realny skan 1500 plików z 1 połączeniem i liczbą bumpów ≤ 2 + sekundy;
    - `commands/scan.rs`: obie komendy trzymają 1 połączenie;
    - `db/thumbnails.rs`: 1 commit na grupę, markery, stale bez zapisu;
    - `thumb_service.rs`: czekający writer `with_scan_and_thumb_lock` nie blokuje nowego strumienia, istniejąca miniatura bez blokady zapisu, failure czyszczone razem ze ścieżką.
  - `bun run rust:fmt` → pass. `bun run rust:clippy` → pass.
  - Desktop E2E: `MEDIATAGGER_GRID_BASELINE=1 MEDIATAGGER_PERF=1 bun run test:e2e:tauri --spec e2e/specs/grid-baseline.e2e.js` → 3 passing.
    - Warunki: prywatny Xvfb 1440×1100, XDG pod `/tmp/mediatagger-*` (tmpfs, tak jak w biegu po etapie 01), `TMPDIR` na dysku (`/var/tmp`).
    - W drzewie był ten etap oraz niezacommitowany frontend etapu 05 z drugiej sesji: zmieniony `DuplicateResolverDialog.tsx`, nowe `gridGeometry.ts` i `createVirtualList.ts`. `bun run typecheck` przechodził. Galeria w tym biegu nadal używała starego wirtualizera.
    - Wynik: like 0/20 i tag 0/20 nieudanych, wszystkie 40 operacji w fazie skanu.
      - `set_asset_tags`: `ipcMs` p50 14, p95 31, max 33 ms (po etapie 01: max 3032 ms).
      - `set_asset_favorite`: `ipcMs` max 32 ms; serce od kliknięcia do UI p50 33, p95 51, max 57 ms (po etapie 01: p95 1629, max 2984 ms).
      - `start_asset_query` i `stale` w czasie skanów: 0 i 0.
      - Skan 20k p50 1269 ms (wcześniej 1364), `remove_scan_root` 162 ms (wcześniej 184).
    - Backend `[perf]`: skan 20k robi 3 bumpy rewizji (wcześniej ~40), 0–4 ustąpienia na skan po ≤ 2 ms. Czekanie na pulę ≤ 0,01 ms. Zapis favorite i tagów w debug max 31–32 ms.
    - Grid i miniatury bez regresji względem bazy: settle p50 17 ms, kafel przy pustym cache p50 361 ms, `render_all_thumbnails` 81 s.
    - Surowe próbki: lokalny, ignorowany `artifacts/grid-baseline/2026-11-06T23-21-11-295Z/results.json`.
- Zostało: commit po akceptacji użytkownika. Kolumnę „Po 01–06” w etapie 07 wypełnia orkiestrator w fali D.
