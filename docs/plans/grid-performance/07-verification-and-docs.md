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
| Rebuild wektora 100k / 1M (ms) | 30 / 377–383 bez filtrów (p50, release, trzy biegi); wykluczenie tagu: 56 / 657–665 | 3,8 / 32 bez filtrów (p50, release, trzy biegi); wykluczenie tagu: 30 / 415–425 | 3,9 / 33–34 bez filtrów (p50, release, trzy biegi); wykluczenie tagu: 30 / 434–438. Sekcje osi czasu bez filtrów, pierwszy odczyt (poza rebuildem): 2,7 / 28,5–28,8; kolejne 0,01–0,02 |
| Strona galerii (ms) | 0,09 / 0,14 w Rust (128 elementów); IPC w E2E (debug, 20k): p50 3, max 6 | 0,10–0,11 / 0,13 w Rust; IPC w E2E: p50 6–7, max 7–8. Binarny zakres ID 1M: 0,44 (bez „Przed”) | 0,10–0,11 / 0,14 w Rust; IPC w E2E: p50 3–7, max 9–10. Binarny zakres ID 1M: 0,46–0,47 |
| Favorite bez skanu / podczas skanu (ms) | 0,02 / p50 4–53, p95 130–429, max 329–529 (Rust, release, trzy biegi); E2E (debug) od kliknięcia do UI: p50 29, p95 196 | 0,02 / p50 1,6–3,2, p95 10–24, max 11–88 (Rust, release, trzy biegi); E2E od kliknięcia do UI: p50 13–20, p95 170–171, max 182–192 | 0,01–0,02 / p50 2,9–4,3, p95 15–21, max 38–59 (Rust, release, trzy biegi); E2E od kliknięcia do UI: p50 24–25, p95 115–151, max 166–176 |
| Nieudane operacje like/tag w scenariuszu (n/20) | like 9/20 (w dwóch wcześniejszych biegach 12/20 i 15/20), zawsze bez wywołania IPC; tag 0/20 | like 0/20, tag 0/20 (dwa biegi E2E); Rust 0 z 292 zapisów pod skanem | like 0/20, tag 0/20 (dwa biegi E2E); Rust 0 z 301 zapisów pod skanem |
| Rebuildy podczas skanu 20k | 43–49 bumpów rewizji na skan. Przy przewijaniu (strona co 100 ms) prawie każda strona jest `stale`: 4–5 rebuildów na skan trwający 0,8–1 s. E2E z otwartym podglądem, bez przewijania: 1 na 66 skanów | Rust: 1 rebuild na skan (30/30 rund); bumpy skanu ≤ 1/s. E2E: 2 bumpy skanu na skan 20k (`[perf] name=scan`), 1 `start_asset_query` i 1 strona `stale` na 19–20 skanów | Rust: 1 rebuild na skan (30/30 rund). E2E: 3 bumpy skanu na skan 20k, 1 `start_asset_query` na 22–34 skany, 0–1 stron `stale` |
| Stop flingu → komplet kafli (ms) | wygasający fling w dół: 16 (jedna klatka); fling w górę: 181; skok paskiem: 192–193 | 16 we wszystkich flingach (także w górę); skok paskiem: 183–194 | 16–17 we flingach (p50); skok paskiem p95 198–199 |
| Najdłuższa klatka po flingu (ms) | 17–25 (odstępy `requestAnimationFrame`; `longtask` niedostępny w WebKitGTK 2.54) | 17 po flingach, 24–25 po skoku paskiem | p50 17, max 27–28 |
| Węzły DOM w gridzie | w spoczynku 379 (63 kafle, 35 widocznych); po flingu 547–589 (91–98 kafli) | stałe 407 węzłów i 98 kafli, w spoczynku i po każdym flingu | stałe 407 węzłów i 98 kafli |
| Viewport → miniatura, pusty cache (ms) | kafel p50 354, p95 444, max 485; komplet widocznych p50 449; kafle z danymi po 180 | kafel p50 287–304, p95 387–396, max 398–430; komplet widocznych p50 405–410; kafle z danymi po 178–184 | kafel p50 297–325, p95 405–426, max 427–472; komplet widocznych p50 412–428; kafle z danymi po 180–183 |
| Miniatura JPEG 24 MP (ms) | 99 (release, `thumbs::create_image_thumb`) | 85 (release, trzy biegi; etap 06 zmierzył 78 w innej sesji) | 62 (release, trzy biegi) |

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

### Warunki pomiaru „Po 01–06” (2026-11-07)

- Ten sam host, wersje WebKitGTK, Rusta i ffmpeg co w „Przed”. Kod z commita `b06a7a0` (etap 06), bez zmian w kodzie aplikacji poza komentarzem w `thumb_service.rs`.
- Backend: ta sama komenda co w „Przed”, trzy biegi, `TMPDIR` na dysku.
- E2E: ten sam spec i flagi, dwa biegi na prywatnym Xvfb 1440×1100. XDG i `TMPDIR` są tymczasowe, na dysku (`/var/tmp`), tak jak w „Przed”.
- Wartości pośrednie z etapów 01–06 są w ich notatkach. Tu jest tylko powtórka na jednym HEAD.

### Warunki pomiaru „Po 08–09” (2026-11-07)

- Ten sam host i wersje co w „Po 01–06”. Kod z commitów `6e10d9a` (etap 08) i `523a8a3` (etap 09), już z poprawkami leniwych sekcji i tożsamości zaznaczenia.
- Backend: ta sama komenda, trzy biegi, `TMPDIR` na dysku. E2E: ten sam spec i flagi, dwa biegi, XDG i `TMPDIR` tymczasowe na dysku (`/var/tmp`).
- **Wiersze E2E z powtórki na bezczynnym hoście (etap 10, 2026-11-07, ok. 21:31–21:37).** Kod z HEAD `76ff5b1` plus niezacommitowane zmiany etapu 10; jedyna zmiana na ścieżce tego specu to `alt` kafla. Monitor obciążenia (próbka co 10 s) nie zarejestrował innej aplikacji: najwięcej CPU brał sam `media_tagger` przy `render_all_thumbnails`, obok przeglądarka (~0,8 rdzenia). `render_all_thumbnails` 85–86 s (fala D ~79 s), skan 20k w E2E p50 874–886 ms (fala D 900–913). Wcześniejsze biegi tej kolumny (dwa z weryfikacji po fali E i dwa z pierwszej próby etapu 10) szły pod obciążeniem gry pod Wine (load 11–21, `render_all_thumbnails` 118–130 s) i zostały odrzucone. Wiersze Rust (release) są z weryfikacji po fali E.

### Wnioski po fali E

- Etap 08 w pierwszej wersji czytał kolumny bucketów dla każdego wiersza rebuildu bez filtrów i podwoił rebuild 1M (32 → 67–69 ms). Wykryte w tej weryfikacji, poprawione: sekcje bez filtrów są leniwe (pierwszy odczyt 28,5–28,8 ms po `ready`, poza ścieżką pierwszej strony), rebuild 1M wrócił do 33–34 ms. Wykluczenie tagu 434–438 ms wobec 415–425 ms w fali D: builder sekcji w tej ścieżce kosztuje ~1,5 ms, reszta mieści się w rozrzucie biegu pod obciążeniem.
- Etap 09: select-all, Shift-click i prostokąt powyżej 10 000 pozycji nie czytają ID; zapis tagu na 1M przez spec 2,8 s, favorite 1,9 s (release, `backend_integration`), wszystko albo nic przy zmianie rewizji. W E2E (12 005 plików) zapis tagu przez spec 388–550 ms, filtr po tagu zgadza się z licznikiem.
- `gallery.performance` znalazł regresję etapu 09: przy zmianie sesji zaznaczenie z samymi ID zmieniało swój klucz tożsamości, przez co panel bulk czyścił szkic grupy po zmianie filtra. Poprawione (bez publikacji przy unieważnieniu samych pozycji) i pokryte testami `createSelection` i `createBulk` (oba padały na starym kodzie).
- Grid bez zmian: 407 węzłów, 98 kafli, harness `gallery-scale` klatki p95 17–18 ms, 0 > 25 ms, 2 strony i 24 żądania `thumb://` na przeciągnięcie, widoczne po 165–200 ms.

### Wnioski po falach A–C

- Like i tag nie zawodzą: 0/20 w obu biegach E2E i 0 z 292 zapisów pod skanem w Rust.
- Zapisy pod skanem: p95 favorite spadło z 130–429 do 10–24 ms (release). W E2E p95 od kliknięcia do UI wynosi ~170 ms. Najwolniejsze próbki wypadają w fazie `remove_scan_root`, która nie ustępuje zapisom użytkownika (znane z etapu 03).
- Rebuild 1M bez filtrów: 377–383 → 32 ms. Z wykluczeniem tagu: 657–665 → 415–425 ms, i to zostaje najdroższy filtr. Skan robi ~2 bumpy rewizji zamiast ~45, więc przewijanie pod skanem prawie nie płaci rebuildów.
- Grid: każdy fling kończy się kompletem w jednej klatce (także w górę, było 181 ms). DOM jest stały: 407 węzłów i 98 kafli. Skok paskiem dalej kosztuje ~185–195 ms, bo strona spoza cache czeka na koniec pauzy (150 ms).
- Zimne miniatury przez `thumb://`: per kafel p50 287–304 ms (było 354), komplet widocznych 405–410 ms (było 449). JPEG 24 MP 85 ms (było 99).

## Otwarte tematy (nie robione w etapie 07)

1. **Scope asset protocol `"**"`** (`src-tauri/tauri.conf.json`). Etap 06 usunął `$APPDATA/thumbs/**`, ale oryginały dalej idą przez scope `**`. Zawężenie do rootów skanu to osobny temat bezpieczeństwa; ADR, gdy zapadnie decyzja.
2. **HiDPI, drugi rozmiar miniatur.** Pominięte decyzją użytkownika w etapie 06. Miniatury mają 390 px, kafle szersze przy DPR > 1 są skalowane w górę (`docs/subsystems/thumbnails.md`).
3. **Usunięcie `list_assets`.** Odłożone decyzją użytkownika. Używa go 17 speców E2E (lista i propozycja helpera w notatce etapu 03 w tym pliku); `gallery.performance` też woła go w `before`.
4. **Bramka wysokości gridu po aktualizacji WebKitGTK.** `MAX_GRID_HEIGHT` 16M pochodzi z bramki 2^24 px w WebKitGTK 2.54.2. Po aktualizacji uruchom `GALLERY_HEIGHT_PROBE=1 sh e2e/gallery-scale/run.sh` (`docs/development/testing.md`, „Scroll height probe”).
5. **Stały spec 100k** dla Shift-click i refreshu głęboko w liście. Pomiary etapu 04 pochodzą z tymczasowego speca spoza repo.
6. **Audyty zależności.** `bun run quality` kończy się na `audit:js`: zod 4.4.3, GHSA-mrhx-c7xc-m3w5 (high, poprawka w 4.6.6). `audit:rust` zgłasza `zip` 2.4.2 (RUSTSEC-2026-0367/0368/0369/0429) i `zstd`/`zstd-safe`/`zstd-sys` (RUSTSEC-2026-0391/0398/0403/0410). To nowe advisory, niezwiązane z etapami 01–06; na czystym HEAD wynik jest taki sam. Advisory `zip` dotyczą `extract`, którego aplikacja nie woła (`backup_service.rs` czyta wpisy przez `by_index` z własną walidacją). Aktualizacja wymaga zmiany lockfile'ów, więc tylko za zgodą użytkownika.
7. **`alt` kafla wideo to pełna ścieżka.** `GalleryTile` używa `preview_path ?? file_name`, a dla wideo `preview_path` jest ścieżką pliku. Czytniki ekranu czytają więc całą ścieżkę. Zachowanie istnieje od migracji na Solid, nie jest regresją.
8. **Pojedyncza wolna próbka w `gallery.performance`.** Rewizyta z gotowymi miniaturami trwa ~90 ms na cel, ale jedna próbka miała 1,6 s (indeks 384). W logu nie ma 503. Do obserwacji przy kolejnych biegach.
9. ~~**Powtórka E2E „Po 08–09” na bezczynnej maszynie.**~~ Zrobione w etapie 10 (warunki wyżej).
10. **Miesiące osi czasu w UTC** ([ADR 0007](../../adr/0007-session-timeline-sections.md)). Lokalna strefa wymagałaby offsetu w kluczu sesji.
11. **Blokada zapisu przy zapisie specu na 1M** ([ADR 0008](../../adr/0008-selection-spec-resolved-in-rust.md)): ~2–3 s jednej transakcji; zapisy w tle czekają w swoim busy timeout. Zbiorowy merge tagów nie kanonizuje starej pisowni tagów każdego assetu.
12. **Brak desktopowego pomiaru przy 1M** dla skoku osi czasu (< 300 ms do danych) i select-all (< 50 ms). Fixture E2E ma 12 005 plików; przy 1M są tylko pomiary Rust (release) i jsdom.
13. **Eval `05-selection-scan` w `.claude/skills/review-changes/evals`** nie nakłada się na `GalleryGrid.tsx` już na HEAD `9428fae` (zmiany etapu 05), więc nie jest to skutek etapów 08–09; wymaga nowej łatki na aktualnym kodzie.

## Status

done — 2026-11-07 (krok 0, fala D i weryfikacja po fali E; wiersze E2E „Po 08–09” do powtórzenia na bezczynnej maszynie)

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

### 2026-11-07 — orkiestrator (fala D)
- Zrobione:
  - commit etapu 06 (`b06a7a0`) przed falą D, za zgodą użytkownika;
  - kolumna „Po 01–06”, warunki pomiaru i wnioski; osobna sekcja „Otwarte tematy”;
  - opt-in specy i harness `gallery-scale` (wyniki niżej);
  - zgodność `.claude/rules/`, `CLAUDE.md` i `docs/` ze źródłem.
- Naprawione regresje (wszystkie w testach, nie w aplikacji):
  - `gallery.performance.e2e.js`: `visibleThumbnailsReady` czekał na `src` kończący się `.jpg`, więc po `thumb://` (etap 06) każdy przypadek padał po 20 s. Warunek: `src` zaczyna się od `thumb://`. Asercja GIF sprawdza URL `thumb://`.
  - `gallery.performance.e2e.js`: przy cache 24 stron (etap 04) fixture 2048 plików (16 stron) nie wymuszał ewikcji. Teraz 3584 pliki (28 stron), pętle liczone z `IMAGE_COUNT`, nowa asercja: strona kotwicy jest pobrana ponownie po przejściu przez wszystkie strony.
  - `e2e/gallery-scale/performance.tsx`: licznik żądań `thumb://` widział tylko zmiany `src` na zamontowanych `<img>`, a kafle tworzone po zatrzymaniu (z gotowym `src`) pomijał, przez co dawał 0. Teraz liczy też wstawione obrazy: 24 żądania na przeciągnięcie, czyli dokładnie widoczne kafle.
- Odkryte:
  - Dokumentacja po etapie 06 dalej opisywała usuniętą kolejkę i store miniatur, „subskrypcje” kafli, kanały miniatur i licznik w stopce. Poprawione:
    - `CLAUDE.md` (Layout, indeks);
    - `docs/architecture/system-overview.md` (diagramy, przepływ IPC, tabela blokad);
    - `docs/frontend/architecture-and-ui-conventions.md`;
    - `docs/subsystems/{library-query-and-gallery,thumbnails,database,settings-operations,lightbox}.md`;
    - `docs/development/testing.md`.
  - `docs/subsystems/thumbnails.md` i komentarz `finish_thumbnail_requests` (`src-tauri/src/services/thumb_service.rs`) twierdziły, że grupowanie commitów oszczędza fsync przy `synchronous=FULL`. Publikator używa zwykłego połączenia WAL z `synchronous=NORMAL` (`db/connections.rs`), więc commit nie robi fsync; grupowanie zmniejsza tylko liczbę blokad zapisu `IMMEDIATE`. Zmiana dotyczy tylko komentarza.
  - `.claude/rules/database.md` i `backend-core.md` były poprawne, ale niepełne. Dopisane: pula 8 z rezerwą 2 na `user_write_connection()` (poll co 1 ms), `yield_to_user_writes()`, `with_thumb_operation` i `thumb_read_section`. Reguła `thumbnails.md` jest zgodna z `app/locks.rs`.
  - `lightbox.md` wskazywał nieistniejące `selectAsset` i `tryCloseLightbox`. Teraz: `select` w `createLightbox.ts` i `tryClose` kontrolek obrazu.
  - Segmentowane przypadki `animations` klikają przez `e2e/native-input.py`, który wymaga `XDG_DATA_HOME` pod `/tmp/mediatagger-*`. Pierwszy bieg z XDG pod `/var/tmp` dał 12 porażek środowiskowych. Dopisane w `testing.md`.
  - `gallery-selection` (2051 plików) i `gallery-browsing` (2048) nie wymuszają już ewikcji przy cache 24 stron; opis w `testing.md` poprawiony. Ewikcję pokrywa `gallery.performance`.
- `/verify-mediatagger` (7 plików syntetycznych):
  - kafle ładują `thumb://localhost/<id>?v=<16 hex>`, wszystkie 7 miniatur widoczne (zrzut), stopka pokazuje tylko koniec wyników;
  - tag i favorite w lightboxie przetrwały przeładowanie;
  - bulk tag na dwóch assetach: wyszukiwanie zwraca dokładnie te dwa;
  - „Rescan all” i „Render thumbnails” dały 7/7, 0 failed;
  - konsola: tylko dwa komunikaty `ResizeObserver loop`.
- Niezweryfikowane przez `/verify-mediatagger`:
  - delete: przycisk w pływającym pasku jest ukryty bez ruchu wskaźnika, a kontroler nie ma komendy ruchu;
  - fling, PageDown, Ctrl+wheel, prostokąt i Shift-click przez duży zakres oraz like/tag podczas długiego skanu: kontroler ma tylko 7 plików. Pokrywają to `grid-baseline`, `gallery-selection`, `gallery.performance` i `gallery-scale`;
  - delete i rename w trakcie generowania miniatur: przy 7 plikach render trwa milisekundy. Pokrywają to tylko testy jednostkowe w `thumb_service.rs`: `stale_render_is_never_served_and_a_retry_renders_again`, `waiting_exclusive_thumb_lock_does_not_stall_new_thumbnail_work` i `publish_updates_drops_everything_after_generation_reset`. Na desktopie pozostaje to niezweryfikowane.
- Zostało: otwarte tematy 1–8 wyżej; weryfikacja po fali E. Commit fali D czeka na akceptację użytkownika.
- Testy:
  - `grid_baseline` (release, `--ignored`) → pass 4/4 w trzech biegach;
  - `grid-baseline.e2e.js` → pass 3/3 w dwóch biegach;
  - E2E pass:
    - `app.smoke` 4/4;
    - `gallery.performance` 4/4 (po poprawkach; pierwszy bieg 3 porażki z powodu `.jpg`);
    - `gallery-selection` 7/7;
    - `ui.redesign` 12/12;
    - `animations` 27/27 (drugi bieg, XDG pod `/tmp/mediatagger-*`);
    - `gallery-browsing` 2/2;
    - `thumbnail-fade` 2/2;
    - `group-order` 1/1;
    - `review-fixes` 3/3;
    - `lightbox-recovery` 2/2;
    - `selection-recovery` 2/2.
  - Ostrzeżenia „element click intercepted” w `group-order` (2) i `review-fixes` (1) są bez zmian względem etapu 05; WebDriver ponawia klik.
  - `sh e2e/gallery-scale/run.sh` → ok (12 scenariuszy, milion wyników, 16M px).
  - `GALLERY_PERFORMANCE=1 sh e2e/gallery-scale/run.sh` → ok: klatki p95 17 ms, 0 > 25 ms, 2 strony i 24 żądania `thumb://` na przeciągnięcie, 0 w pauzie, widoczne miniatury 166–201 ms po zatrzymaniu.
  - `bunx tsc --noEmit -p e2e/gallery-scale/tsconfig.json` → pass.
  - `bun run test` → 817/817; `bun run test:backend` → 261 pass, 6 ignored.
  - `bun run quality` → locale, typecheck, lint, format, rustfmt i clippy pass. `audit:js` i `audit:rust` fail na nowych advisory (temat 6), tak samo na czystym HEAD.
  - `bun run privacy:check`: test etapu 06 w `thumb_protocol.rs` miał syntetyczną ścieżkę w katalogu domowym. Skaner nie widział jej, póki plik był nieśledzony, a po pierwszym commicie etapu 06 zgłosił ją jako prywatną ścieżkę. Ścieżkę zmieniłem na `/srv/media/a.jpg` i poprawiłem commit etapu 06 (amend, przed wypchnięciem). `cargo test thumb_protocol` → 5/5; `privacy:check` → pass.

### 2026-11-07 — orkiestrator (weryfikacja po fali E)
- Zrobione: kolumna „Po 08–09”, warunki pomiaru i wnioski po fali E, otwarte tematy 9–13; status etapów 08 i 09.
- Odkryte i poprawione w kodzie etapów 08–09 (przed commitem):
  - rebuild 1M bez filtrów 32 → 67–69 ms przez odczyt kolumn bucketów w etapie 08; teraz sekcje bez filtrów są leniwe (szczegóły w etapie 08);
  - regresja etapu 09 w `gallery.performance`: zmiana sesji zmieniała tożsamość zaznaczenia z samymi ID i czyściła szkic grupy (szczegóły wyżej);
  - harness `e2e/gallery-scale` używał usuniętego propsa `selectedAssetIds` (`fixture.tsx`, `performance.tsx`); przeniesiony na `isSelected`.
- Testy:
  - `grid_baseline` (release, `--ignored`, trzy biegi) → pass 4/4 w każdym;
  - `grid-baseline.e2e.js` → pass 3/3 w dwóch biegach (pod obciążeniem hosta);
  - E2E pass: `gallery.performance` 4/4 (drugi bieg, po poprawce; pierwszy 3/4 na regresji wyżej), `gallery-selection` 7/7, `selection-recovery` 2/2, `gallery-browsing` 2/2, `gallery-timeline` 3/3; `gallery-selection`, `selection-recovery` i `gallery-browsing` szły przed poprawką tożsamości zaznaczenia (zmiana dotyczy tylko zmiany sesji przy zaznaczeniu z samymi ID);
  - `sh e2e/gallery-scale/run.sh` → ok (12 scenariuszy, 16M px); `GALLERY_PERFORMANCE=1 …` → ok, klatki p95 17–18, 0 > 25 ms, widoczne po 165–200 ms; `bunx tsc --noEmit -p e2e/gallery-scale/tsconfig.json` → pass;
  - `bun run test` → 843/843; `bun run test:backend` → pass; typecheck, lint, format:check, locale:check, `cargo fmt -- --check`, clippy → pass; `git diff --check` czysto, `--numstat` identyczny z `--ignore-cr-at-eol`.
- Niezweryfikowane: porównywalne czasy E2E „Po 08–09” (temat 9); desktop przy 1M (temat 12); `/verify-mediatagger` dla osi czasu i Ctrl+A (siedem fixture'ów ma jedną datę, więc oś się nie pokazuje; Ctrl+A pokrywa `gallery-timeline`).

### 2026-11-07 — orkiestrator (etap 10)
- Z etapu 10: tematy 5, 7, 12 i 13 zamknięte (szczegóły w [10](10-open-topics.md)). Desktop przy 1M (`GALLERY_MILLION=1 sh e2e/gallery-scale/run.sh`, transport syntetyczny): skok End do najstarszego miesiąca 21–27 ms do kafli z danymi, Ctrl+A do licznika 1–3 ms bez odczytu ID. Stały spec 100k (`MEDIATAGGER_GALLERY_100K=1`): Shift-click 10k 10–11 ms (1 binarny odczyt), 100k 2–3 ms (0), refresh na 49 987 jednym zapytaniem z kotwicą, bez placeholderów.
- Z etapu 10: temat 9 dalej otwarty. Próba powtórki 2026-11-07 ok. 21:11–21:18 znowu pod obciążeniem (gra pod Wine wystartowała minutę po starcie pierwszego biegu, load 18–21, `render_all_thumbnails` 118–122 s), więc wiersze (\*) zostają bez zmian.
- Z etapu 10: temat 9 zamknięty. Druga próba (ok. 21:31–21:37) na bezczynnym hoście, z monitorem obciążenia: oba biegi `grid-baseline.e2e.js` 3/3. Wiersze (\*) zastąpione, warunki pomiaru zaktualizowane. Względem „Po 01–06”: like/tag 0/20, DOM 407/98, settle 16 ms, skok paskiem p95 ~200 ms, cold per kafel p50 297–325 ms (było 287–304); bez regresji po etapach 08–09.
