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
todo — 2026-11-04

## Notatki agentów
<!-- dopisuj wpisy według protokołu z README -->
