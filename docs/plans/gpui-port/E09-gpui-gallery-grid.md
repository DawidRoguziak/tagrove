# E09: Wirtualna siatka galerii i miniatury

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E06, E07.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Płynna siatka kafli dla biblioteki do 1 000 000 pozycji, z miniaturami z istniejącego schedulera.

## Zakres

- Geometria siatki w kliencie (czysta); wirtualne wiersze w GPUI. Limit 16 000 000 px ze strony WebKit nie jest potrzebny, ale logiczne przesunięcie musi być stabilne przy 1 000 000 pozycji.
- Kafel: miniatura, znacznik ulubionego, chip „GIF”/„Video” (D8), opóźniony spinner, wejście przez opacity.
- Ładowanie miniatur: ograniczony cache zdekodowanych obrazów, tylko-cache podczas szybkiego przewijania, prefetch dla overscanu, ponowienia z backoffem (ADR 0024), widoczny kafel porzuca oczekujący prefetch (ADR 0021), dekodowanie poza wątkiem UI.
- Pusty stan, stopka statusu, suwak rozmiaru kafli (domyślnie 188) i Ctrl+kółko.
- Aktywacja kafla zgłasza „otwórz lightbox” (obsługa w E16).
- Wpisy w `HOT_PATHS` (`collect_scope.py`) dla plików siatki i miniatur w `src-gpui/` + test skryptu.

## Poza zakresem

- Pasek przewijania i scrubber (E10), zaznaczanie (E11).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/gallery/GalleryGrid.tsx`
- `src/features/gallery/createVirtualGrid.ts`
- `src/features/gallery/gridGeometry.ts`
- `src/features/gallery/GalleryTile.tsx`
- `src/features/gallery/GalleryEmptyState.tsx`
- `src/features/gallery/GalleryStatusFooter.tsx`
- `src/features/gallery/TileSizeSlider.tsx`
- `src/features/gallery/tileSizeService.ts`
- `src/ui/controls/ThumbnailImage.tsx`
- `src/app/AppGalleryView.tsx`

Testy do przeniesienia:
- `src/features/gallery/__tests__`
- `src/ui/controls/__tests__`

Dokumenty i reguły:
- `docs/subsystems/library-query-and-gallery.md`
- `docs/subsystems/thumbnails.md`
- `docs/adr/0003-custom-gallery-virtualizer.md`
- `docs/adr/0021-visible-tiles-drop-a-pending-prefetch.md`
- `docs/adr/0024-thumbnail-retries-continue-while-the-tile-is-mounted.md`

## Kryteria akceptacji

- Testy geometrii przeniesione; test „widoczny zakres → żądane strony”.
- Test górnej granicy pamięci cache obrazów.
- Pomiar na syntetycznej bibliotece ≥ 100 000 pozycji (fixture jak w `grid_baseline`/`e2e/gallery-scale`): czas klatki przy przewijaniu zapisany w notatkach.
- Parytet `gallery-default`, `gallery-tiles-small`, `gallery-tiles-large`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E09 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`
- `python3 -m unittest discover -s .claude/skills/review-changes/scripts -p test_collect_scope.py`

## Scenariusze porównania

- `gallery-default`: Seed: obrazy PNG + 1 GIF + 2 wideo; domyślny rozmiar kafli; widoczny pasek przewijania i scrubber.
- `gallery-tiles-small`: Ten sam seed, najmniejszy rozmiar kafli.
- `gallery-tiles-large`: Ten sam seed, największy rozmiar kafli.

## Ryzyka

- Dekodowanie JPEG na wątku UI; nieograniczony cache tekstur.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E05

- Z epiku E05: spinner kafla to `ui::spinner::Spinner::new(id, SpinnerSize::Tile).reveal(THUMBNAIL_LOADER)` (300 ms niewidoczny, potem 150 ms ease-out, `media_tagger_client::controls`); animacja startuje przy pierwszym narysowaniu elementu o danym id. Chip rodzaju mediów: `ui::chip::Chip` z `ChipTone::Video`/`Gif`. Suwak rozmiaru kafli: encja `ui::slider::Slider` (kontrolowana, `SliderEvent::Input`).

### 2026-11-21 — implementer E07

- Z epiku E07: siatka czyta wiersze przez `runtime.model().query()`: `total()` to liczba pozycji do rozłożenia, `asset_at(indeks)` zwraca wiersz z cache albo wiersz zastępczy po odświeżeniu (tylko do rysowania), `version()` rośnie przy każdej zmianie cache. Nie indeksuj `assets()` jak listy wyników (to tylko wiersze z cache, najwyżej 24 strony).
- Z epiku E07: zakres zgłaszaj przez `RuntimeInput::Query(QueryInput::SetViewport(Some(Viewport { start_index, end_index, visible_start_index, visible_end_index, paused })))`, a przy zniknięciu galerii `SetViewport(None)`. `start..=end` to widoczne wiersze z overscanem, nigdy cały wynik: model żąda każdej strony zakresu (strony po 128) i nie ogranicza liczby żądań. `paused: true` (szybkie przewijanie) zapamiętuje zakres i nie żąda stron; wznowienie to ten sam zakres z `paused: false`. `visible_start_index` jest kotwicą następnego odświeżenia. Retry banera błędu to `QueryInput::RetryLoad`; stan błędu: `load_error()` i `page_failure_epoch()`.
