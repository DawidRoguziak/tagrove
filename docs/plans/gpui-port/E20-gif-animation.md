# E20: Animacja GIF w lightboxie

Rozmiar: M. Poziom: standardowy. Zależy od: E16.

**Odroczony.** Nie uruchamiać workflow bez nowej decyzji użytkownika; przed startem powstaje osobny plan tego epiku.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Zastąpić zaślepkę GIF animacją.

## Zakres

- Sprawdzić natywną animację elementu obrazu GPUI; limit pamięci klatek; pauza poza widokiem.

## Poza zakresem

- brak

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/lightbox/LightboxMediaStage.tsx`

Testy do przeniesienia:
- brak

Dokumenty i reguły:
- `docs/subsystems/lightbox.md`

## Kryteria akceptacji

- GIF animuje się w lightboxie; pamięć ograniczona dla długich animacji.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E20 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `lightbox-gif`: Lightbox z GIF-em (GPUI: zaślepka, D8).

## Ryzyka

- brak

## Status

deferred

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
