# E14: Dok akcji zbiorczych i kolejność grup

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E11, E13.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Pływający dok dla zaznaczenia: tagi oraz grupa i kolejność (ADR 0035).

## Zakres

- Model `createBulk` i `createSelectedSummaries` w kliencie; serwisy scalania tagów i kolejności grup.
- Dok: tablist z ruchomym tabindexem (strzałki, Home/End); zakładka tagów (jeden zasób: zamiana, wiele: tylko dodawanie); zakładka grupy (klucz, generowany UUID, zastosuj/usuń, komunikat o konflikcie).
- `GroupOrderBoard`: przeciąganie do zmiany kolejności, strzałki, góra/dół o wiersz; inline albo w modalu, nigdy oba naraz.

## Poza zakresem

- brak

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/bulk`
- `src/features/lightbox/parseMediaGroupOrder.ts`

Testy do przeniesienia:
- `src/features/bulk/__tests__`

Dokumenty i reguły:
- `docs/subsystems/search-tags-and-media-groups.md`
- `docs/adr/0035-bulk-actions-floating-dock.md`
- `docs/adr/0026-set-based-ranged-group-write-with-index-rebuild.md`

## Kryteria akceptacji

- Testy modeli i serwisów bulk przeniesione.
- Częściowy wynik publikuje tylko potwierdzone ID.
- Parytet `bulk-dock-tags`, `bulk-dock-group`, `group-order-modal`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E14 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `bulk-dock-tags`: Dok, zakładka tagów, wiele zaznaczonych zasobów.
- `bulk-dock-group`: Dok, zakładka grupy i kolejności z planszą inline.
- `group-order-modal`: Plansza kolejności w modalu.

## Ryzyka

- Przeciąganie z klawiaturą: łatwo zgubić fokus po zmianie kolejności.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
