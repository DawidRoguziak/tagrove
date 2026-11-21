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

done 2026-11-21

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E05

- Z epiku E05: spinner kafla to `ui::spinner::Spinner::new(id, SpinnerSize::Tile).reveal(THUMBNAIL_LOADER)` (300 ms niewidoczny, potem 150 ms ease-out, `media_tagger_client::controls`); animacja startuje przy pierwszym narysowaniu elementu o danym id. Chip rodzaju mediów: `ui::chip::Chip` z `ChipTone::Video`/`Gif`. Suwak rozmiaru kafli: encja `ui::slider::Slider` (kontrolowana, `SliderEvent::Input`).

### 2026-11-21 — implementer E07

- Z epiku E07: siatka czyta wiersze przez `runtime.model().query()`: `total()` to liczba pozycji do rozłożenia, `asset_at(indeks)` zwraca wiersz z cache albo wiersz zastępczy po odświeżeniu (tylko do rysowania), `version()` rośnie przy każdej zmianie cache. Nie indeksuj `assets()` jak listy wyników (to tylko wiersze z cache, najwyżej 24 strony).
- Z epiku E07: zakres zgłaszaj przez `RuntimeInput::Query(QueryInput::SetViewport(Some(Viewport { start_index, end_index, visible_start_index, visible_end_index, paused })))`, a przy zniknięciu galerii `SetViewport(None)`. `start..=end` to widoczne wiersze z overscanem, nigdy cały wynik: model żąda każdej strony zakresu (strony po 128) i nie ogranicza liczby żądań. `paused: true` (szybkie przewijanie) zapamiętuje zakres i nie żąda stron; wznowienie to ten sam zakres z `paused: false`. `visible_start_index` jest kotwicą następnego odświeżenia. Retry banera błędu to `QueryInput::RetryLoad`; stan błędu: `load_error()` i `page_failure_epoch()`.

### 2026-11-21 — workflow gpui-port-epic

- Zrobione: moduł `gallery` w crate klienta bez GUI (`geometry`, `virtual_grid::VirtualGrid`, `thumbnail_demand::ThumbnailDemand`, `picture_cache::PictureCache`, `layout`); w adapterze encja `Thumbnails` (`thumbs.rs`, dekodowanie na executorze tła) oraz widoki `views/gallery.rs` i `views/gallery/grid.rs` (kafle, kółko, Ctrl+kółko, klawisze przewijania, zastępniki, podkłady grup, stopka, baner błędu z Retry, stan pusty). Scenariusze `gallery-default`, `gallery-tiles-small`, `gallery-tiles-large`, seed kontrolny przez `Backend::scan_folder`. ADR 0042, `HOT_PATHS` rozszerzone, strony docs zaktualizowane. Testy: przeniesiono 57 przypadków (`tests/gallery_ported.rs`, `mod ported` w `tests/gallery_grid.rs`), dodano 3; naprawiono 2 błędne testy (Retry przy przewiniętej siatce, limit kafli zależny od okna).
- Odkryte: (1) ADR 0042: siatka nie jest kontenerem przewijania GPUI; jedno logiczne przesunięcie f64, fizyczne okno Solid (MAX_GRID_HEIGHT, base, rebase) nie ma odpowiednika. (2) Renderowanie tylko czyta; każda zmiana stanu idzie przez `GalleryView::refresh`, bo GPUI gubi notify w trakcie klatki; rozmiar viewportu mierzony w prepaint, układ spóźnia się o klatkę, pierwsza klatka nie rysuje kafli. (3) E10 używa tego samego modelu: `GridInput::ScrollTo`, `JumpToIndex`, `HoldDemand`, `ResumeDemand`; zarezerwowano 34 px po prawej na oś czasu. (4) E11/E16: aktywacja kafla to `GalleryEvent::OpenAsset { asset_id, index }`, nikt jeszcze nie subskrybuje; id kafla `("gallery-tile", index)`, selektor `gallery::tile_selector(index)`. (5) `Thumbnails` należy do `Runtime`, obrazy przeżywają wizytę w ustawieniach. Budżet `PictureCache` 256 MiB, obrazy w zakresie przypięte. (6) Brak znacznika ulubionych i zanikania obrazu: nie ma ich w kaflu Solid (ADR 0042); wymaga decyzji planu, jeśli mają być. (7) Klawisze przewijania w `shortcuts::SHORTCUTS`, kontekst `Gallery`; nie działają, gdy fokus na korzeniu okna. Kafle nie są tab stopami, brak Enter/Space i pierścienia fokusu (E11/E19). (8) `Scenario` ma `seed` i `tile_size`; `MainScreen::Shell` niesie rozmiar kafli; stan pusty przesunięty o 7 px w lewo jak w Tauri (zmieniono test w `shell_window.rs`). (9) `ThumbnailInput` dostał `PartialEq`. (10) Pominięte testy TS: mechanika fizycznego okna WebKit, recentrowanie, DOM slot recycling, przechwytywanie kółka, selekcja (E11), pasek przewijania i scrubber (E10), fade obrazu, HTMLImageElement complete, kontrolki (E05); nazwa dostępna kafla i opis grupy (`gallery.groupedChip`) nieprzetestowalne, bo test platform GPUI nie buduje drzewa a11y, a kafel GPUI chyba nie ustawia opisu grupy. (11) Parity, otwarte drobne: kolejność kafli przy równych kluczach sortowania różni się między sesjami (też w Tauri), warto sprawdzić tiebreak; brak paska przewijania w gallery-tiles-large (E10); brak elementów nagłówka i paska narzędzi z E12/E13/E14. (12) Pomiar klatki (build release, 100 000 wyników, 600 klatek, test platform, tylko CPU): 188 px kafle 72 kafle średnia 1,44 ms, p95 3,95 ms; 140 px 126 kafli średnia 2,67 ms, p95 8,82 ms; 140 px pauza 0,69 ms, max 29,08 ms. Komenda: `cargo test --manifest-path src-gpui/Cargo.toml --locked --release -p media_tagger_gpui --test gallery_grid frame_time -- --ignored --nocapture`.
- Zostało: nic nie blokuje; brak otwartych uwag przeglądu (2 rundy, 1 potwierdzona i naprawiona); bramka przeszła w 1 rundzie. Do zrobienia poza zakresem: pomiar klatki w prawdziwym oknie z GPU, sprawdzenie hover ringu i Wayland, aktualizacja `CLAUDE.md` i `.claude/rules/` o nowe moduły galerii (użytkownik), pozostałe zestawy testów TS galerii poza przeniesionymi. Przegląd: w rundzie 1 brakowało listy plików zakresu (soczewki dobrane bez niej).
- Testy:
  - cargo fmt --check (src-gpui): pass
  - cargo clippy --all-targets --all-features --locked -D warnings (src-gpui): pass
  - cargo test -p media_tagger_client (w tym gallery_ported 31): pass
  - cargo test -p media_tagger_gpui (w tym gallery_grid 33, pomiar klatki ignorowany): pass
  - architecture_boundaries (33), bun run gpui:lock-parity, bun run test:compare (50), bun run lint, test_collect_scope.py (14): pass
  - compare:ui dla trzech scenariuszy galerii: pass (różnice drobne, opisane wyżej)
  - bun run test:all (bramka): pass
