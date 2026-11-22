# E10: Pasek przewijania i scrubber osi czasu

Rozmiar: M. Poziom: standardowy. Zależy od: E09.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Własny pasek przewijania i miesięczny scrubber obok siatki.

## Zakres

- Pasek przewijania: przeciąganie, strzałki, PageUp/PageDown, Home/End.
- Scrubber osi czasu z sekcji sesji (ADR 0007); stan „niedostępne” zamiast nieaktualnych sekcji (ADR 0027); własne klawisze.
- Etykiety dostępności z i18n.

## Poza zakresem

- brak

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/gallery/GalleryScrollbar.tsx`
- `src/features/gallery/GalleryTimelineScrubber.tsx`
- `src/features/gallery/timeline.ts`

Testy do przeniesienia:
- `src/features/gallery/__tests__`

Dokumenty i reguły:
- `docs/subsystems/library-query-and-gallery.md`
- `docs/adr/0007-session-timeline-sections.md`
- `docs/adr/0027-unfiltered-sections-unavailable-instead-of-stale.md`

## Kryteria akceptacji

- Testy `timeline` przeniesione.
- Klawiatura paska i scrubbera jak w Solid.
- Parytet `gallery-default` (scrubber widoczny).
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E10 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `gallery-default`: Seed: obrazy PNG + 1 GIF + 2 wideo; domyślny rozmiar kafli; widoczny pasek przewijania i scrubber.

## Ryzyka

- brak

## Status

done 2026-11-22

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-22 — workflow gpui-port-epic
- Zrobione: pasek pozycji galerii i scrubber osi czasu (miesiące) w GPUI; oba crate'y się kompilują. Crate klienta: `gallery/timeline.rs` (port `timeline.ts`: scalanie miesięcy, wyszukiwanie miesiąca, pozycja szyny na indeks, cele klawiszy, etykiety, przerzedzone znaczniki lat, model `TimelineScrubber`), `gallery/scrollbar.rs` (kciuk min. 32 px, klik obok kciuka, przeciąganie, klawisze), `i18n/date.rs` (etykiety miesięcy jak Intl.DateTimeFormat w 11 językach, sprawdzone względem ICU z Node). Adapter: `views/gallery/position.rs` (pasek 10 px, szyna 24 px od prawej krawędzi, role i etykiety dostępności). Przeciąganie wstrzymuje ładowanie stron i miniatur i stosuje tylko ostatni cel raz na klatkę. Brak sekcji (ładowanie, błąd, nieaktualne, ADR 0027) ukrywa szynę. Testy: przeniesiono 13 przypadków TS i dodano 23 testy. Zaktualizowano dokumenty: library-query-and-gallery, testing, system-overview, architecture-and-ui-conventions, localization.
- Odkryte: pasek GPUI jest zawsze własny (`GalleryScrollbar`), gdy treść się przewija; Tauri pokazuje cienki pasek WebKitGTK, więc `gallery-tiles-large` pokaże różnicę. Pasek mapuje cały zakres przewijania VirtualGrid, nie tylko wiersze. Bez nowych bindingów: fokusowany pasek i szyna używają istniejących akcji Gallery (Shift+strzałka na szynie nic nie robi, brak aria-controls). Szyna to model klienta z `TimelineInput`/`TimelineEffect`, a skok stosowany przez `GridInput::JumpToIndex`. Kolejność tabulatora: pasek, szyna, Retry. Etykiety miesięcy to zahardkodowane dane CLDR w `i18n/date.rs`, więc nowy język wymaga też tam miesięcy i wzorca (localization.md, krok 7). Selektory: gallery-scrollbar, gallery-scroll-thumb, gallery-timeline, gallery-timeline-marker, gallery-timeline-label, gallery-timeline-tick-{rok}. Pominięte przypadki testowe: atrybuty aria (testowa platforma GPUI nie włącza drzewa dostępności; sprawdzono wartości na modelu), defaultPrevented, setPointerCapture i shimy rAF (mechanika DOM/jsdom, zastąpione testami zachowania). Kryterium „parytet gallery-default (scrubber widoczny)" nie może zajść: seed ma 7 plików z jednego miesiąca i mieści się w oknie, więc oba programy pokazują brak paska i scrubbera; potrzebny seed wielomiesięczny w e2e/compare/ (poza zakresem E10). Parytet, różnice drobne (minor): kolejność 4 kafli PNG (E09/E07), brak pola wyszukiwania w pasku tytułu (E12), brak filtra rodzaju, ulubionych i listy tagów (E12/E13), brak przycisku Bulk actions (E14), nagłówek końca wyników nieco grubszy (E09). Nie rozszerzono HOT_PATHS kolektora zakresu przeglądu o nowe pliki galerii. Sekcja Status, tabela README i NOTES.md nie były ruszane przez implementera.
- Zostało: nic nie blokuje. Błąd testu (prawy przycisk na szynie kradnie fokus) został naprawiony w pętli sprawdzeń (1 poprawka); ten sam wzorzec dotyczył paska. Przegląd: 4 rundy, czysto, 5 potwierdzonych i naprawionych, 2 odrzucone, brak otwartych; raportu nie utworzono. Bramka: przeszła w 1 rundzie. Niesprawdzone na prawdziwym ekranie: hover i wygląd; brak testu kolejności tabulatora; hover szyny nietestowany na platformie testowej GPUI.
- Testy:
  - rust:fmt / cargo fmt --check: przeszło
  - rust:clippy / clippy -D warnings (oba workspace'y): przeszło
  - architecture_boundaries: przeszło
  - gpui:lock-parity: przeszło
  - test:client (572 testy jednostkowe plus integracyjne, w tym gallery_position_ported, 14): przeszło
  - test:gpui (w tym gallery_position): przeszło po poprawce
  - compare:ui gallery-default: uruchomione, różnice tylko drobne (patrz Odkryte)
  - test:all (bramka): przeszło

### 2026-11-22 — agent naprawczy luk E10 (wygląd paska i szyny)
- Zrobione: zastępcze porównanie wyglądu (jak plansza `kit` w E05): pasek i szyna narysowane rendererem headless w obu motywach przy 100 000 wynikach z 60 miesięcy, piksele zmierzone względem klas `GalleryScrollbar.tsx` i `GalleryTimelineScrubber.tsx`. Naprawiono etykietę miesiąca: 11,5 px wysokości zamiast 26 px, bo widok brał stosunek `TYPOGRAPHY.xs.line_height` (16/12) za piksele (src-gpui/crates/media_tagger_gpui/src/views/gallery/position.rs:393, :407; `label_height` i `label_line_height` w src-gpui/crates/media_tagger_client/src/gallery/timeline.rs:534, :540). Nowy test `tests/gallery_position_render.rs` (pomija się bez renderera, chyba że ustawiono `MEDIATAGGER_GPUI_REQUIRE_RENDERER`), asercja wysokości etykiety w `tests/gallery_position.rs:618` i test jednostkowy w timeline.rs:812. Raport: reports/E10-review-open.md. Zaktualizowano testing.md i library-query-and-gallery.md.
- Odkryte: poza etykietą wszystko zgodne z klasami Solid (położenie, tor `base-content/5` i `/10`, kciuk `--border-strong` min. 32 px, znacznik `primary/70`, znaczniki lat `base-content/30`, lata `base-content/60` tylko pod kursorem, etykieta `--surface-solid` z ramką i cieniem, pierścień 2 px `primary` 2 px od elementu, tylko po klawiaturze). Zrzut Tauri `gallery-tiles-large` potwierdza, że kafle kończą się na x 1405, a nakładkowy pasek WebKitGTK nie ma rynny, więc odległości od prawej krawędzi są te same. Różnice świadome: pasek GPUI zawsze własny (ADR 0042), fonty poza parytetem pikselowym (D10).
- Zostało: porównanie z działającą aplikacją Tauri wymaga seedu wielomiesięcznego w e2e/compare/ (decyzja planu; propozycja seedu `timeline` i scenariusza `gallery-timeline` w raporcie). Najechanie prawdziwą myszą w oknie GPUI niesprawdzone (test wysyła zdarzenia przez `Window::dispatch_event`).
- Testy:
  - `MEDIATAGGER_GPUI_REQUIRE_RENDERER=1 cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test gallery_position --test gallery_position_render --test kit` → pass (bez poprawki: gallery_position i gallery_position_render padają)
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client` → pass (575)
  - `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`, `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`, `git diff --check` → pass
  - `job.py start E10 checks` → pass (8 kroków, w tym test:client i test:gpui z gallery_position_render bez pominięcia)
