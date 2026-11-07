# Etap 05: własny virtualizer gridu w Solid 2

Fala A. Warstwa: TypeScript `src/features/gallery`, `src/ui/controls/ThumbnailImage.tsx`. Decyzja użytkownika: piszemy własny od razu i usuwamy `@tanstack/virtual-core`.

Dokumentacja do przeczytania:
- `docs/subsystems/library-query-and-gallery.md` (virtual gallery);
- `docs/frontend/architecture-and-ui-conventions.md`;
- `docs/development/testing.md` (testy 10k/26k/1M slotów);
- reguła `.claude/rules/gallery-query.md`.

## Problemy obecnej implementacji (potwierdzone w kodzie)

1. **Szybki scroll odmontowuje wszystkie kafle.**
   - `src/features/gallery/createVirtualGrid.ts:34-43` ustawia `paused`, gdy krok ≥ ½ viewportu albo suma z 100 ms ≥ viewport.
   - `src/features/gallery/GalleryGrid.tsx:246` (`when={!virtual.paused && assetAtSlot()}`) odmontowuje wtedy wszystkie kafle, także te z danymi.
   - Po 150 ms wszystkie (100–500 przy 4K) montują się w jednej klatce i placeholdery migają.
2. **Każdy event scrolla przebudowuje wszystko.**
   - `createVirtualGrid.ts:105` robi `version++`, co przebudowuje `snapshot` (`:202-228`, obiekt na komórkę).
   - Potem kolejno: diff `<For>` z nową tablicą (`GalleryGrid.tsx:227`), `entries` (`:104-108`), `backplates` (`:109-142`) i `start()` każdego kafla (`:240-243`), bo czyta `scrollShift`.
3. **Tryb skompresowany** (`src/features/gallery/scrollCoordinates.ts`, `MAX_GRID_HEIGHT = 4_000_000`):
   - shift zmienia się co klatkę, więc ~500 transformów pisanych z JS;
   - `preventDefault` na wheel (`createVirtualGrid.ts:140-149`) wyłącza płynny scroll WebKit;
   - `scrollTo` zapisuje i czyta `scrollTop`, co wymusza layout.
4. **`translate3d` na każdym kaflu i backplate** może tworzyć setki warstw kompozycji w WebKitGTK. Do sprawdzenia przez `WEBKIT_SHOW_COMPOSITING_DEBUG_VISUALS=1`.
5. **`ThumbnailImage`** (`src/ui/controls/ThumbnailImage.tsx:44`, `Show keyed`) tworzy nowy `<img>` przy każdej zmianie `src`. `GalleryTile` zawsze renderuje SVG zaznaczenia i nie ma `decoding="async"`.

## Projekt

### Moduł geometrii (czysty TS, bez DOM)
`gridGeometry.ts`: wejście `{width, tileMin, gap, count, viewport, margin}`, wyjście:
- `columns`, `size`, `stride = size + gap`, `rows = ceil(count / columns)`, `contentHeight`;
- `rowRange(offset, overscanRows)` → `{first, last, visibleFirst, visibleLast}`;
- `rowTop(row)`, `indexToRow(index)`, `anchorForResize(prevGeometry, offset)`.

Wszystko O(1). Testy jednostkowe dla 10k, 26k i 1M (granice, ostatni niepełny wiersz, overscan na krańcach).

### Model scrolla
- **Bramka pomiarowa (pierwszy krok etapu).** W E2E, na prawdziwym WebKitGTK, zmierz, przy jakiej wysokości kontenera layout, `scrollTop` i transformy przestają być poprawne. Twardy limit LayoutUnit to ~33,5M px. Cel: podnieść `MAX_GRID_HEIGHT` z 4M do ~16M (z zapasem). Pomiar zapisz w notatkach.
- **Poniżej limitu:** natywny scroll (`overflow: auto`) i spacer o wysokości `contentHeight`. Bez przechwytywania wheel.
- **Powyżej limitu:** własny model.
  - Offset logiczny jako `number` (double) w JS jest źródłem prawdy. Scroller ma `overflow: hidden`.
  - Własny komponent paska przewijania, dostępny z klawiatury: `role="scrollbar"`, `aria-valuenow`, przeciąganie, klik w tor.
  - Wheel, touchpad (`deltaMode`), klawisze PageUp, PageDown, Home, End i strzałki obsługujemy w JS.
  - Treść przesuwana jednym `transform` na wewnętrznym wrapperze.
- Mapowanie `toLogical`/`toPhysical` w `scrollCoordinates.ts` usuwamy. Ten sam interfejs `scrollTo` i `scrollBy` obsługuje oba tryby.
- Scrubber z etapu 08 podepnie się pod ten sam model (`scrollToIndex`).

### Render
- **Pula wierszy:** `poolSize = visibleRows + 2 × overscanRows` (~20–30). Wiersz `r` trafia do slotu `r % poolSize`. Slot to `<div class="row">` z `transform: translate(0, Ypx)` (2D, nie `translate3d`), `contain: strict`, o stałej wysokości. Komórki układa CSS grid (`grid-template-columns: repeat(columns, size)`).
- Przejście przez wiersz zmienia tylko `rowIndex` slotu. Komórki dostają nowy `index`, a ich memo pobiera `getAssetAt(index)`. **Bez tworzenia komponentów** podczas scrolla, dopóki nie zmieni się liczba kolumn.
- Scroll handler: odczyt `scrollTop` (lub offsetu modelu) w `requestAnimationFrame`, potem `rowRange`. Sygnał zakresu zmienia się **tylko przy zmianie wierszy**. W trybie natywnym nie ma `version++` per event.
- Brak odczytów layoutu z DOM w ścieżce scrolla. Geometria (`width`, `margin`, `viewport`) pochodzi tylko z `ResizeObserver`.
- **Pauza przy szybkim scrollu wstrzymuje tylko popyt:** `onRange`, czyli `ensureRange` i miniatury. Kafle z danymi zostają zamontowane. Placeholder tylko dla slotów bez danych.
- **Backplates grup:** liczone per wiersz (wiersz zna swoje indeksy), renderowane wewnątrz wiersza.
- **`ThumbnailImage`:** jeden `<img>` na komórkę. Zmiana `src` podmienia atrybut, a stary obraz ukrywa klasa do czasu `load`. Dodaj `decoding="async"`. Zachowaj `crossorigin="anonymous"` (powód: `docs/subsystems/thumbnails.md`).
- **`GalleryTile`:** SVG zaznaczenia renderowany warunkowo, mniej sygnałów i efektów per kafel.

### Zachowanie (bez regresji)
- API dla `createGridHandlers`: `gridOffset`, `columnCount`, `tilePixelSize`, `tileGap`, `scrollShift` (0 w trybie natywnym), `scrollBy`. Zaznaczanie prostokątem i Shift-click liczą pozycje matematycznie przez `selectionGeometry.ts`.
- Kotwica przy resize i zmianie rozmiaru kafla (pierwszy widoczny indeks zostaje na górze), Ctrl+wheel zoom, nawigacja klawiaturą, focus.
- Atrybuty dla E2E: `data-asset-index` na komórce, `data-scroll-paused` i `data-testid="gallery-grid"` na sekcji. Jeśli zmieni się DOM lub aria, zaktualizuj specy w `e2e/specs` i recepty skilla `verify-mediatagger`.

## Kroki
1. Bramka pomiarowa limitu wysokości (E2E przez orkiestratora; agent przygotowuje stronę testową lub spec).
2. `gridGeometry.ts` z testami.
3. Nowy `createVirtualGrid.ts` (ten sam eksport, nowa implementacja): model scrolla, sygnał zakresu, pauza tylko dla popytu.
4. Nowy `GalleryGrid.tsx`: pula wierszy, CSS grid, backplates per wiersz.
5. `GalleryTile.tsx` i `ThumbnailImage.tsx`.
6. Własny pasek przewijania dla trybu powyżej limitu.
7. Usunięcie `@tanstack/virtual-core`: `bun remove @tanstack/virtual-core` (zmiana zależności, więc zmiana `bun.lock` jest tu zamierzona), aktualizacja `CLAUDE.md` (Stack), reguły i dokumentacji.
8. Porównanie przed i po (etap 07): klatki podczas flingu, czas „stop → wszystkie kafle”.

## Pliki
- `src/features/gallery/createVirtualGrid.ts`, `GalleryGrid.tsx`, `GalleryTile.tsx`, `scrollCoordinates.ts` (do usunięcia lub zastąpienia), nowe `gridGeometry.ts` i `GalleryScrollbar.tsx`
- `src/features/gallery/createGridHandlers.ts`, `selectionGeometry.ts` (tylko jeśli zmieni się API)
- `src/ui/controls/ThumbnailImage.tsx`
- `package.json`, `bun.lock`
- `docs/subsystems/library-query-and-gallery.md`, `docs/frontend/architecture-and-ui-conventions.md`, `docs/development/testing.md`, `.claude/rules/gallery-query.md`, `CLAUDE.md`

## Testy
- `src/features/gallery/__tests__/gridGeometry.test.ts` (nowy): 10k, 26k, 1M, resize i kotwica.
- `createVirtualGrid.test.tsx` i `GalleryGrid.test.tsx` przepisane:
  - szybki scroll **nie odmontowuje** kafli z danymi (licznik montowań);
  - zmiana zakresu tylko przy zmianie wiersza;
  - liczba węzłów DOM stała podczas scrolla;
  - tryb powyżej limitu: wheel, klawiatura i pasek;
  - backplates grup.
- `createGridHandlers.test.tsx`, `selectionGeometry.test.ts`, `scrollCoordinates.test.ts` (usunąć lub zastąpić).
- `bun run test -- src/features/gallery src/ui/controls`, `bun run typecheck`, `bun run lint`.
- Desktop E2E (orkiestrator): smoke i specy galerii.

## Kryteria akceptacji
- Fling i PageDown bez migania placeholderów dla załadowanych danych. Brak długiej klatki po zatrzymaniu (cel < 16 ms na typowej liczbie kafli; pomiar w etapie 07).
- Liczba warstw kompozycji ≈ liczba wierszy, nie liczba kafli.
- Płynny natywny scroll do co najmniej ~16M px. Powyżej limitu działa pasek, wheel i klawiatura.
- Brak `@tanstack/virtual-core` w `package.json`.

## Ryzyka
- Utrata zachowań, które dziś „przypadkiem” daje virtual-core (debounce `isScrolling`, korekty offsetu). Mitygacja: przenieś istniejące testy kotwicy jako testy regresji przed przepisaniem.
- Własny model scrolla powyżej limitu: inercja touchpada w WebKitGTK przychodzi jako seria eventów wheel. Sprawdź na prawdziwym sprzęcie przez `verify-mediatagger`.

## Status
done — 2026-11-07 (commit czeka na akceptację użytkownika)

## Notatki agentów
<!-- dopisuj wpisy według protokołu z README -->

### 2026-11-06 — orkiestrator (etap 07, krok 0)
- Z etapu 07: wartości bazowe obecnego gridu (`e2e/specs/grid-baseline.e2e.js`, 20 243 elementy, okno 1440×1000, Xvfb, ciepłe miniatury):
  - wygasający fling w dół (2–19 tys. px): komplet kafli jedną klatkę po zatrzymaniu; pauza wirtualizera aktywna przez 121–173 ze 137–194 klatek szybkich flingów;
  - fling w górę i skoki paskiem: 181–193 ms od zatrzymania do kompletu;
  - najdłuższa klatka w sekundzie po zatrzymaniu: 17–25 ms (rAF; `longtask` niedostępny);
  - DOM gridu: 379 węzłów i 63 kafle w spoczynku, 547–589 węzłów i 91–98 kafli po flingu.
- Uwaga do kryterium „< 16 ms”: pomiar idzie przez odstępy rAF na programowym renderowaniu Xvfb, gdzie jedna klatka to ~16,7 ms. Porównuj z bazą (17–25 ms), nie z wartością bezwzględną.

### 2026-11-07 — agent etapu 05 (krok 1: bramka wysokości)
- Zrobione: strona `e2e/gallery-scale/height-probe.{html,tsx}` (`GALLERY_HEIGHT_PROBE=1 sh e2e/gallery-scale/run.sh`). To czysty DOM w prawdziwym WebKitGTK 2.54.2 (ta sama biblioteka co w Tauri) na prywatnym Xvfb, z produkcyjnym CSS `.gallery-scroll`.
  - Dla wysokości 4M–64M sprawdzałem: `scrollHeight`, zapis i odczyt `scrollTop` (także nieparzyste i ułamkowe), pozycję markera z 2D `translate` + `contain: strict`, `elementFromPoint`, krok natywnego wheel (XTest) i malowanie (piksel ze screenshotu).
  - Bieg przy `GDK_SCALE=2` (`devicePixelRatio = 2`) na ekranie 3800×1800.
- Odkryte:
  - Do **16 777 000 px** (poniżej 2^24) wszystko jest dokładne i marker się maluje.
  - Od **16 800 000 px** marker przy końcu jest przesunięty o 1 px (float32) i **w ogóle się nie maluje**, choć layout i hit-testing działają.
  - Ta sama granica w px CSS przy skali 1 i 2, więc nie zależy od skali urządzenia.
  - Treść saturuje na 33 554 428 px (limit LayoutUnit).
  - Krok wheel: 88–89 px.
  - Zapis `scrollTop = Number.MAX_SAFE_INTEGER` daje 0: offsety trzeba przycinać w JS.
  - Natywny suwak: `.gallery-scroll` ma standardowe `scrollbar-width`/`scrollbar-color`, więc reguły `::-webkit-scrollbar-*` nie działają.
    - `scrollbar-color: transparent` zostawia gutter 21 px, a klik w gutter nadal skacze.
    - `scrollbar-width: none` usuwa gutter (+21 px szerokości) i jego obsługę.
- Decyzja: `MAX_GRID_HEIGHT = 16 000 000`, czyli ~0,78M px zapasu do 2^24 na margines, stopkę i panel bulk pod galerią.
  - Tryb okna ukrywa natywny pasek przez `scrollbar-width: none` i oddaje gutter jako `padding-right`, żeby liczba kolumn się nie zmieniała.
- Pomiary „before” na starym gridzie (ignorowane `artifacts/gallery-scale/`):
  - `performance-before-05.json`: p95 klatki 17 ms, max 17–18 ms, obraz po 224–240 ms od zatrzymania, 80–81 klatek pauzy na przeciągnięcie, 2 strony i 1 batch miniatur na przeciągnięcie;
  - `scale-before-05.json`: wszystkie scenariusze 1M przechodzą przy spacerze 4M.
  - Pierwszy bieg „before” szedł równolegle z `cargo test` drugiej sesji, więc odrzuciłem go (`performance-before-05-under-cargo-load.json`).
  - Powtórka i wszystkie pomiary bramki szły bez cargo/rustc/vitest/wdio. W drzewie były wtedy niezacommitowane zmiany Rust etapu 02, które nie dotyczą tego harnessu (bez Tauri).
- Testy: `GALLERY_HEIGHT_PROBE=1 sh e2e/gallery-scale/run.sh` → ok (skala 1 i 2); `bunx tsc --noEmit -p e2e/gallery-scale/tsconfig.json` → pass.

### 2026-11-07 — agent etapu 05 (kroki 2–8)
- Zrobione:
  - `gridGeometry.ts`: czysta geometria O(1), `MAX_GRID_HEIGHT = 16 000 000` z bramki.
  - Nowy `createVirtualGrid.ts`: wiersze publikowane razem z layoutem i tylko przy zmianie wierszy; scroll czyta wyłącznie `scrollTop`; pauza wstrzymuje tylko popyt.
  - `GalleryGrid.tsx`: pula slotów wierszy w kolejności DOM = kolejność wizualna, 2D `translate` + `contain: strict`, CSS grid, backplates per wiersz.
  - `GalleryTile` bez pozycjonowania, znacznik zaznaczenia tylko przy `selected`; `ThumbnailImage` z jednym `<img>` i `decoding="async"`.
  - Powyżej limitu tryb okna (decyzja użytkownika zamiast `overflow: hidden`): natywny scroll w oknie od `base`, rebase w bezruchu albo awaryjnie przy krawędzi, Home/End na `window`, `GalleryScrollbar.tsx` (`role="scrollbar"`). Natywny pasek ukryty przez `data-gallery-windowed` + `scrollbar-width: none` + oddany gutter.
  - Dialogi przeniesione na `src/ui/dom/createVirtualList.ts`. `bun remove @tanstack/virtual-core`: diff `bun.lock` dotyczy tylko tego pakietu.
  - [ADR 0003](../../adr/0003-custom-gallery-virtualizer.md), dokumentacja (galeria, frontend, settings, bulk, setup, testing), reguła `gallery-query`, `CLAUDE.md`, recepta `verify-mediatagger`, specy E2E (`tile.parentElement` → `[data-testid="gallery-grid-content"]`), fixture `e2e/gallery-scale`.
  - Nowy klucz `gallery.scrollbarLabel` w 11 locale.
- Odkryte:
  - Solid 2 `For` przy rotacji puli o jeden wiersz przesuwa dwa węzły, oba poza ekranem. Przeniesienie węzła zdejmuje fokus, stąd obsługa Home/End także dla `body`.
  - Gettery czytające `view()` budziły efekt w `createGridHandlers.ts` przy każdej zmianie wierszy, a jego cleanup (`cancelForLayout`) przerywał przeciąganie prostokąta, gdy edge-scroll przesunął grid. Teraz to memo, z testem regresji.
  - Pierwszy pomiar szerokości po montażu przewijał o margines. Poprawione: kotwica tylko gdy offset > 0, test regresji.
  - Ostrzeżenia WebDrivera „element click intercepted” w `group-order` i `review-fixes` są też w starym pełnym biegu z 2026-10-03, więc nie są regresją.
- Wyniki `grid-baseline.e2e.js` (Xvfb 1440×1100, okno 1440×1000, debug, `TMPDIR` na dysku), przed → po:
  - stop → komplet kafli: 16 ms dla wszystkich flingów (fling w górę był 181–193 ms); skoki paskiem 180–181 ms (181–193), bo dane spoza cache czekają na koniec pauzy i stronę;
  - najdłuższa klatka po zatrzymaniu: 17–20 ms (17–27);
  - DOM gridu: stałe 211 węzłów i 98 kafli (379 i 63 w spoczynku, 547–589 i 91–98 po flingu);
  - zimny cache bez zmian: kafle z danymi 173 ms, komplet miniatur p50 482 ms;
  - like i tag 0/20 nieudanych.
- Wyniki `GALLERY_PERFORMANCE=1` (26k, natywny drag), przed → po: p95 klatki 17 ms → 17 ms, max 17–18 ms → 17–18 ms, obraz po zatrzymaniu 224–240 → 224–236 ms, 2 strony i 1 batch miniatur bez zmian. W drzewie podczas pomiarów był już zacommitowany etap 02 (`acb6054`), bez niezacommitowanego Rusta. Maszyna wolna (bez cargo, rustc, vitest, wdio).
- Zostało:
  - etap 04 usuwa `hasMore`/`onReachEnd` (interfejs zostawiony);
  - etap 06 (`thumb://`) korzysta z nowego `ThumbnailImage`;
  - etap 08 podpina scrubber pod `scrollToIndex`/`offset`.
  - Bramki nie powtarzałem na prawdziwym touchpadzie (ryzyko z pliku etapu): tryb okna zostawia inercję natywną, a rebase nie pisze `scrollTop` w trakcie ruchu, chyba że do krawędzi okna zostały 2 viewporty.
- Testy:
  - `bun run test -- src/features src/ui src/app src/library src/i18n src/utils src/platform` → 821/821 pass;
  - `bun run typecheck`, `bun run lint`, `bun run format:check`, `bun run locale:check`, `bun run test:build`, `bunx tsc --noEmit -p e2e/gallery-scale/tsconfig.json`, `git diff --check` → pass;
  - `sh e2e/gallery-scale/run.sh` (1M: tryb okna, pasek, End/Home, recenter, wheel, zaznaczanie) → pass; `GALLERY_PERFORMANCE=1 …` → pass;
  - desktop E2E na ostatecznym kodzie: `gallery-selection`, `selection-recovery`, `gallery-browsing`, `settings-maintenance` → 4/4 pass; `gallery.performance`, `group-order`, `review-fixes`, `lightbox-recovery`, `thumbnail-fade` → 5/5 pass; `grid-baseline` (`MEDIATAGGER_GRID_BASELINE=1 MEDIATAGGER_PERF=1`) → 3/3 pass.
  - Wcześniejsze biegi pośrednie wykryły dwa opisane wyżej błędy; po poprawkach przechodzą.
