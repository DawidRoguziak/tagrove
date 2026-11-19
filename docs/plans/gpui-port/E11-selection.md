# E11: Model zaznaczenia i gesty

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E08, E09.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Zaznaczenie jako specyfikacja rozwiązywana w Rust, z tymi samymi gestami co dziś.

## Zakres

- Model w kliencie: zakresy sesji + ID włączone/wyłączone, epoki zapytania i tożsamości, `keepAlive`, rozwiązywanie zakresów do 10 000 pozycji.
- Gesty w GPUI: klik, Ctrl/Meta-klik, Shift-zakres, zaznaczanie przeciąganiem po 150 ms przytrzymania, Ctrl+A, przełącznik trybu zaznaczania.
- Mutacja selekcji przez port (`apply_selection_mutation`).

## Poza zakresem

- Dok akcji zbiorczych (E14).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/gallery/createSelection.ts`
- `src/features/gallery/selection.ts`
- `src/features/gallery/createGridHandlers.ts`
- `src/features/gallery/selectionGeometry.ts`

Testy do przeniesienia:
- `src/features/gallery/__tests__`

Dokumenty i reguły:
- `docs/subsystems/search-tags-and-media-groups.md`
- `docs/adr/0008-selection-spec-resolved-in-rust.md`
- `docs/adr/0029-range-selection-keeps-its-session-alive.md`

## Kryteria akceptacji

- Testy `createSelection` i `selectionGeometry` przeniesione.
- Zaznaczenie przeżywa odświeżenie zapytania zgodnie z epokami; porażka zapytania je unieważnia.
- Parytet `gallery-selection`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E11 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `gallery-selection`: Trzy zaznaczone kafle (klik, Ctrl-klik, Shift-klik), widoczny dok.

## Ryzyka

- Rozwijanie dużych zakresów do list ID po stronie UI.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
