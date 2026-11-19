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

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
