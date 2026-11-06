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

todo — 2026-11-04

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
