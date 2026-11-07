# Etap 07: pomiary, weryfikacja end-to-end i zgodność dokumentacji

Wykonuje orkiestrator. Krok 0 jest **przed** falą A, reszta w fali D (po etapach 01–06), a ponowna weryfikacja po fali E.

Dokumentacja do przeczytania:
- `docs/development/testing.md` (wybór warstwy testu, izolacja E2E, flagi `MEDIATAGGER_*`);
- skill `verify-mediatagger`.

## Krok 0: pomiary bazowe (przed zmianami)

Wszystko na **syntetycznych** danych w katalogach tymczasowych. Nigdy nie używaj prywatnych folderów ani prawdziwych danych aplikacji.

1. **Generator fixture'ów.**
   - Skrypt (np. w `scripts/` albo jako test diagnostyczny `#[ignore]`) tworzy bazę 100k i 1M wierszy w `tempfile`, z grupami, tagami i favorites.
   - Do E2E: folder tymczasowy z ~5–20k małych wygenerowanych JPEG i kilkoma krótkimi wideo (ffmpeg `testsrc`).
2. **Backend** (`MEDIATAGGER_PERF=1`):
   - czas `start_asset_query` (rebuild wektora) dla 100k i 1M, bez filtrów i z filtrami tag i favorite;
   - czas `get_asset_query_page`;
   - czas zapisu favorite i tagu bez skanu i podczas skanu;
   - liczba rebuildów podczas skanu.
3. **Frontend** (E2E na Xvfb, `verify-mediatagger`):
   - czas od zatrzymania flingu do kompletu kafli;
   - najdłuższa klatka po zatrzymaniu (`PerformanceObserver` `longtask`, jeśli dostępny w WebKitGTK; w przeciwnym razie pętla rAF);
   - liczba węzłów DOM w gridzie;
   - czas od wejścia kafla w viewport do pojawienia się miniatury przy pustym cache.
4. **Scenariusz błędu like i tagów:** skan 20k plików, w trakcie 20 kliknięć serca i dodań tagu. Licznik nieudanych i opóźnionych operacji.
5. Wyniki wpisz do sekcji „Wyniki” poniżej: liczby, bez ścieżek i nazw prywatnych plików.

## Fala D: weryfikacja po etapach 01–06

1. Powtórz pomiary z kroku 0 i uzupełnij tabelę.
2. Pełne bramki jakości:
   - `bun run quality`;
   - `bun run test` (jeśli zabraknie pamięci: `-- --maxWorkers=1`);
   - `bun run test:backend`;
   - `bun run privacy:check && bun run test:privacy`;
   - `git diff --check`.
3. Desktop E2E, **jeden po drugim**:
   - `e2e/specs/app.smoke.e2e.js`;
   - specy galerii, lightboxa i bulk (pełna lista w `docs/development/testing.md`, „Real desktop E2E”, z wymaganymi flagami `MEDIATAGGER_<FLAG>=1`).
4. `verify-mediatagger`:
   - fling i PageDown;
   - Ctrl+wheel zoom;
   - zaznaczanie prostokątem;
   - Shift-click przez duży zakres;
   - like i tag w lightboxie oraz w bulk podczas skanu;
   - delete i rename podczas generowania miniatur.

   Zrzuty ekranu tylko z danych syntetycznych.
5. **Zgodność dokumentacji.** Każda strona zmieniona przez etapy opisuje aktualne zachowanie, a odnośniki i komendy działają:
   - `docs/subsystems/library-query-and-gallery.md`, `thumbnails.md`, `database.md`, `search-tags-and-media-groups.md`, `scanning-and-indexing.md`, `lightbox.md`, `data-safety-and-portability.md`;
   - `docs/architecture/ipc-contract.md`, `system-overview.md`;
   - `docs/frontend/architecture-and-ui-conventions.md`;
   - `docs/development/testing.md`;
   - `.claude/rules/*.md`, `CLAUDE.md` (Stack: brak TanStack virtual-core).
6. Zaktualizuj statusy etapów i NOTES.md. Zbierz listę tego, co pozostało niezweryfikowane.

## Wyniki

| Metryka | Przed | Po 01–06 | Po 08–09 |
| --- | --- | --- | --- |
| Rebuild wektora 100k / 1M (ms) | 30 / 377–383 bez filtrów (p50, release, trzy biegi); wykluczenie tagu: 56 / 657–665 | | |
| Strona galerii (ms) | 0,09 / 0,14 w Rust (128 elementów); IPC w E2E (debug, 20k): p50 3, max 6 | | |
| Favorite bez skanu / podczas skanu (ms) | 0,02 / p50 4–53, p95 130–429, max 329–529 (Rust, release, trzy biegi); E2E (debug) od kliknięcia do UI: p50 29, p95 196 | | |
| Nieudane operacje like/tag w scenariuszu (n/20) | like 9/20 (w dwóch wcześniejszych biegach 12/20 i 15/20), zawsze bez wywołania IPC; tag 0/20 | | |
| Rebuildy podczas skanu 20k | 43–49 bumpów rewizji na skan. Przy przewijaniu (strona co 100 ms) prawie każda strona jest `stale`: 4–5 rebuildów na skan trwający 0,8–1 s. E2E z otwartym podglądem, bez przewijania: 1 na 66 skanów | | |
| Stop flingu → komplet kafli (ms) | wygasający fling w dół: 16 (jedna klatka); fling w górę: 181; skok paskiem: 192–193 | | |
| Najdłuższa klatka po flingu (ms) | 17–25 (odstępy `requestAnimationFrame`; `longtask` niedostępny w WebKitGTK 2.54) | | |
| Węzły DOM w gridzie | w spoczynku 379 (63 kafle, 35 widocznych); po flingu 547–589 (91–98 kafli) | | |
| Viewport → miniatura, pusty cache (ms) | kafel p50 354, p95 444, max 485; komplet widocznych p50 449; kafle z danymi po 180 | | |
| Miniatura JPEG 24 MP (ms) | 99 (release, `thumbs::create_image_thumb`) | | |

### Warunki pomiaru „Przed” (2026-11-06)

- Host: AMD Ryzen 7 9800X3D (16 wątków), 30 GiB RAM, NVMe, btrfs na LUKS, Linux (CachyOS). WebKitGTK 2.54.2, Rust 1.98.0, Node 26.7.0, Bun 1.4.0, ffmpeg n9.0.2. Kod aplikacji z commita `04bb342` (Rust bez zmian względem `ad96392`).
- Backend: `TMPDIR=/var/tmp cargo test --manifest-path src-tauri/Cargo.toml --locked --release --test grid_baseline -- --ignored --nocapture --test-threads=1`. Dane i scenariusze opisuje `docs/development/testing.md`, sekcja „Grid-performance baseline”.
- E2E: `MEDIATAGGER_GRID_BASELINE=1 MEDIATAGGER_PERF=1 bun run test:e2e:tauri --spec e2e/specs/grid-baseline.e2e.js` na prywatnym Xvfb 1440×1100 (okno 1440×1000, DPR 1, renderowanie programowe). Katalogi XDG i `TMPDIR` są tymczasowe, na dysku. Build E2E jest debug, więc praca backendu w tym specu (miniatury, skan, IPC) idzie w niezoptymalizowanym Ruście.
- Kolumna „Przed” w wierszach E2E pochodzi z biegu weryfikacyjnego ostatecznej wersji specu. W nawiasach jest rozrzut z dwóch wcześniejszych biegów tego samego scenariusza. Surowe próbki leżą lokalnie w ignorowanym `artifacts/grid-baseline/<znacznik czasu>/results.json`.
- Skan 20 000 małych plików trwa 0,8–1 s (release) i ~1,5 s (debug E2E), czyli krócej niż 20 operacji przez WebDriver. Dlatego scenariusz E2E przeplata `scan_folder` z `remove_scan_root` tego samego folderu, a każda operacja ma zapisaną fazę.

### Wnioski z pomiaru bazowego

- Like zawodzi we frontendzie: klik w serce, gdy zapis tagu na tym samym assecie jest w toku, jest odrzucany bez IPC (UI: „The favorite change was not saved”). Backend nie odrzucił żadnego zapisu (0 z 224 w trzech biegach Rust, 0 błędów IPC w E2E). To przyczyna 1 z etapu 01.
- Zapisy podczas skanu są wolniejsze, ale nie zawodzą: max ~0,3–0,7 s w release, czekanie na połączenie z puli ≤ 0,6 ms. Wyczerpania puli przy tym obciążeniu nie zaobserwowano.
- Rebuild przy 100k jest tani (30 ms), przy 1M kosztuje 377–665 ms zależnie od filtra. Przy ~45 bumpach na skan 20k przewijający użytkownik płaci rebuild prawie przy każdej stronie (etapy 02, 03, 04).
- Ciepły grid: wygasający fling kończy się kompletem w jednej klatce. Skok i szybki fling w górę kosztują ~180–190 ms, co odpowiada 150 ms progowi pauzy wirtualizera (`createVirtualGrid.ts`) plus ~30–40 ms. Po flingu w gridzie jest 91–98 kafli i 547–589 węzłów (etap 05).
- Zimne miniatury: ~350–450 ms na kafel w buildzie debug. Pełne `render_all_thumbnails` dla 20 243 plików 320×240 trwa 82 s w debug (etap 06).

## Status

in progress — 2026-11-06 (krok 0 zrobiony; fala D i weryfikacja po fali E czekają)

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->

### 2026-11-06 — orkiestrator (krok 0)
- Zrobione: generator danych i pomiary bazowe.
  - `src-tauri/tests/grid_baseline.rs`: trzy diagnostyki `#[ignore]` (rebuild i strony dla 100k/1M; zapisy favorite/tag bez skanu i podczas skanów 20k z symulowanym przewijaniem; miniatura JPEG 24 MP).
  - `e2e/specs/grid-baseline.e2e.js` (`MEDIATAGGER_GRID_BASELINE=1`): like/tag pod obciążeniem skanem, fling i DOM przy ciepłym cache, viewport → miniatura przy pustym cache.
  - Opis w `docs/development/testing.md` („Grid-performance baseline”, tabela flag) i `grid_baseline` na liście targetów w `CLAUDE.md`. Kolumna „Przed” w tabeli wyników.
- Odkryte:
  - Błąd like odtworzony w prawdziwej aplikacji, przyczyna we frontendzie (szczegóły w etapie 01).
  - Skan 20k małych plików trwa 0,8–1 s (release) i ~1,5 s (debug E2E), dlatego scenariusz E2E przeplata skan z usuwaniem folderu.
  - Wiersze bez mapowania w `asset_scan_roots` znikają przy zakończonym skanie dowolnego folderu (`prune_completed_scan_root_generation`, `src-tauri/src/db/scan_membership.rs:267`). To zgodne z dokumentacją, ale syntetyczne bazy muszą mieć mapowanie.
  - WebKitGTK 2.54 nie udostępnia `longtask`; klatki mierzone są przez `requestAnimationFrame`.
  - `render_all_thumbnails` trwa dłużej niż limit skryptu WebDrivera. WDIO ponawia `executeAsync`, a ponowienie kończy się „Thumbnail render is already running”. Długie komendy uruchamia się w stronie i odpytuje o wynik (`invokeLong` w specu).
  - Chip tagu w podglądzie pojawia się od razu jako zamiar w toku, więc potwierdzenie zapisu tagu liczy się z IPC, nie z UI.
- Zostało: fala D (pomiary po 01–06, bramki jakości, desktop E2E, zgodność dokumentacji) i weryfikacja po fali E. Commit kroku 0 czeka na akceptację użytkownika.
- Testy:
  - `TMPDIR=/var/tmp cargo test --manifest-path src-tauri/Cargo.toml --locked --release --test grid_baseline -- --ignored --nocapture --test-threads=1` → pass 3/3 (cztery biegi, ostatni na ostatecznym kodzie); bez `--ignored` → 3 ignored.
  - `MEDIATAGGER_GRID_BASELINE=1 MEDIATAGGER_PERF=1 bun run test:e2e:tauri --spec e2e/specs/grid-baseline.e2e.js` na prywatnym Xvfb → pass 3/3 w biegach 2–4. Bieg 1 padł na `render_all_thumbnails` (poprawione) i miał scenariusz bez ciągłego skanu.
  - `bun run rust:fmt`, `bun run rust:clippy`, `bun run lint`, `bun run format:check`, `bun run privacy:check` → pass; `git diff --check` → czysto.

### 2026-11-06 — agent etapu 01
- Z etapu 01: bieg pośredni `grid-baseline.e2e.js` po etapie 01 (tylko frontend, przed 02–06): like 0/20 i tag 0/20 nieudanych; serce od kliknięcia do UI p50 84 ms, p95 1629 ms, max 2984 ms. Kolumny „Po 01–06” nie wypełniam, bo jest na pomiar po falach A–C. Uwaga: ten bieg miał XDG i `TMPDIR` na tmpfs.

### 2026-11-07 — agent etapu 02
- Z etapu 02: pomiary pośrednie po etapie 02, przed 03–06. Kolumny „Po 01–06” nie wypełniam. Szczegóły i warunki są w notatkach etapu 02.
  - `grid_baseline` (release, ten sam host, przed i po w jednej sesji):
    - favorite pod skanem: p95 129 → 24 ms, max 179 → 35 ms;
    - tag pod skanem: p95 529 → 19 ms, max 639 → 31 ms;
    - rebuildy: 4–5 → 1 na skan;
    - nieudane operacje: 0.
  - `grid-baseline.e2e.js` (debug; XDG na tmpfs, `TMPDIR` na dysku):
    - like i tag 0/20 nieudanych;
    - `set_asset_tags` max 33 ms, serce do UI max 57 ms;
    - 0 rebuildów i 0 stron `stale` w czasie skanów;
    - 3 bumpy rewizji na skan 20k.
- Z etapu 02: metryka `revision_bumps` w `grid_baseline.rs` liczy też bumpy samych zapisów użytkownika (favorite bumpuje zawsze). Bumpy skanu widać w `[perf] name=scan revision_bumps=…`.
- Z etapu 02: do sprawdzenia zgodności w fali D:
  - `.claude/rules/thumbnails.md` („render/ensure take the `thumb_lock` read guard”): teraz guard per krótka sekcja przez `thumb_read_section`;
  - `.claude/rules/database.md` (5 s busy timeout): zapis użytkownika ma ten sam budżet, ale odpytuje co 1 ms;
  - `.claude/rules/backend-core.md`: zapisy użytkownika używają `user_write_connection()`.

  Strony `docs/` i ADR 0002 są już zaktualizowane.

### 2026-11-07 — agent etapu 05
- Z etapu 05: pomiar pośredni po falach 01, 02 i 05, przed 03, 04 i 06. Kolumny „Po 01–06” nie wypełniam. `grid-baseline.e2e.js` (debug, Xvfb 1440×1100, XDG pod `/tmp`, `TMPDIR` na dysku, maszyna wolna):
  - stop → komplet kafli: 16 ms we wszystkich flingach, także w górę (było 181–193 ms); skoki paskiem 180–181 ms, bo dane spoza cache wciąż czekają na koniec pauzy (150 ms) i stronę;
  - najdłuższa klatka po zatrzymaniu: 17–20 ms;
  - DOM gridu: stałe 211 węzłów i 98 kafli;
  - zimny cache: kafle z danymi 173 ms, komplet p50 482 ms;
  - like i tag 0/20 nieudanych.
- Z etapu 05: bramka limitu wysokości jest powtarzalna przez `GALLERY_HEIGHT_PROBE=1 sh e2e/gallery-scale/run.sh` (opis w `docs/development/testing.md`, „Scroll height probe”). Po aktualizacji WebKitGTK uruchom ją ponownie.
- Z etapu 05: przy porównaniach fixture `GALLERY_PERFORMANCE=1` „przed/po” trzeba robić dwoma biegami na dwóch commitach (podmiana `GALLERY_BASELINE_DIR` usunięta). Wyniki etapu 05 leżą w ignorowanym `artifacts/gallery-scale/`: `performance-before-05.json`, `performance-after-05.json`, `scale-before-05.json`, `scale-after-05.json`, `height-probe-scale{1,2}.json`.
- Z etapu 05: do przeglądu w fali D zostają ostrzeżenia WebDrivera „element click intercepted” w `group-order` i `review-fixes`. Występowały już w pełnym biegu z 2026-10-03, a specy przechodzą.

### 2026-11-07 — agent etapu 03
- Z etapu 03: pomiary pośrednie po etapach 01, 02, 05 i 03, przed 04 i 06. Kolumny „Po 01–06” nie wypełniam. Szczegóły w notatkach etapu 03.
  - `grid_baseline.rs` (release, p50 przy 1M): rebuild bez filtrów 384 → 32 ms, exclude_common 671 → 420 ms, favorite pod skanem p50 4,3 → 2,0 ms;
  - nowa diagnostyka `sort_key_backfill_plan_and_bulk_group_baseline`: backfill przy upgrade ~1,5 s, plan bez filtrów, bulk 5000 do jednej grupy 146–171 ms;
  - `grid-baseline.e2e.js` (debug), dwa biegi: skan 20k p50 695 ms (było ~1266 ms), like i tag 0/20 nieudanych. Pojedyncze zapisy 120–180 ms wypadają w fazie `remove_scan_root`, która nie ustępuje zapisom użytkownika; to ograniczenie istniało już wcześniej.
- Z etapu 03: usunięcie `list_assets` odłożone (decyzja użytkownika). Do zrobienia w osobnej zmianie:
  - komenda w `commands/assets.rs` i `lib.rs`, wrapper `listAssets` z testem w `src/platform/tauri/__tests__/api.test.ts`, typ w `contracts/`, `ipc-contract.md`;
  - funkcje `list_assets`/`list_assets_with_meta` w `db/queries.rs` oraz ich testy w `db.rs`, `backend_integration.rs`, `backend_e2e.rs` i `scan_service.rs`;
  - 17 speców E2E: startup-scan, search-recovery, lightbox-images, lightbox-recovery, group-order, app.workflows, ui.redesign, settings-maintenance, gallery.performance, native-video-escape, thumbnail-fade, animations, video-speed, lightbox-activity, review-fixes, app.smoke, popular-tags.

  Propozycja: helper w `e2e/` (np. `listAssetsForAssertions`), który w przeglądarce woła `start_asset_query` z własnym `ownerId` (np. `e2e-assertions`) i rosnącym `generation`, pobiera strony po 256 przez `get_asset_query_page`, a pola `path`/`tags`/`size_bytes` (których `AssetSummary` nie ma dla obrazów) przez `get_asset_details`. Zwraca `{ items, total }` w kształcie `Asset`. Uwaga: start z innego ownera przerywa niedokończony start aplikacji (globalny token żądania), więc helper wołać, gdy galeria jest spokojna.

### 2026-11-07 — agent etapu 04
- Z etapu 04: frontend trzyma teraz 24 strony (`MAX_CACHED_PAGES`). `gallery.performance.e2e.js` (2048 plików, 16 stron) przechodzi, ale jego „evicted page” już nie wymusza ewikcji; przy weryfikacji końcowej trzeba albo powiększyć fixture (> 24 stron), albo opisać to jako ograniczenie.
- Z etapu 04: pomiary etapu 04 (refresh głęboko w liście, Shift-click 100k) dopiszę po fazie B.
- Z etapu 04 (faza B): `grid_baseline.rs` (`query_rebuild_and_page_baseline`) ma nową metrykę `get_asset_query_id_range` (release: 100k 0,05 ms, 1M 0,45 ms). Desktopowe pomiary Shift-click 100k (15–17 ms) i refreshu na indeksie 50 000 (1 IPC, 0 placeholderów) pochodzą z tymczasowego speca spoza repo; wyniki i metoda są w notatkach etapu 04. Do rozważenia przy weryfikacji końcowej: stały spec z kolekcją 100k.
- Z etapu 04 (faza B): `grid-baseline` po etapie 04: skan 20k p50 691 ms, like/tag 0/20 nieudanych, settle p50 17 ms, 211 węzłów; bez regresji względem biegu etapu 03.

### 2026-11-07 — agent etapu 06
- Z etapu 06: kolumna „Po 01–06”, `grid-baseline` cold (debug): per kafel p50 286 / p95 389 ms (przed 354 / 444), komplet widocznych p50 403 ms (przed 449); warm bez zmian (settle p50 16 ms).
- Z etapu 06: miniatura JPEG 24 MP 78 ms (przed 98), wideo 1080p GOP 250 48 ms (przed 202).
- Z etapu 06: harness `e2e/gallery-scale` rejestruje syntetyczny schemat `thumb` w `webkit.py`, a liczba żądań miniatur to teraz zmiany `src` na `thumb://`; liczby z poprzednich biegów („ensure_thumbnails batches”) nie są porównywalne.
