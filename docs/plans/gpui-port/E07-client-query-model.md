# E07: Model zapytań i cache stron

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E02.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Port `createAssetQuery`: sesje zapytań i cache stron dla wyników do 1 000 000 pozycji.

## Zakres

- Generacja i owner ID dla każdego zapytania; przyjmowana tylko odpowiedź `ready` bieżącej generacji.
- `stale` → odświeżenie po 250 ms, nie częściej niż raz na sekundę, z `anchorOffset` = pierwszy widoczny indeks.
- Strony po 128, cache LRU na 24 strony, licznik `version` przy każdej zmianie cache.
- Sekcje osi czasu, pozycja zasobu po odświeżeniu, zakresy ID, `keepAlive` dla zaznaczenia (ADR 0029), epoka porażek.
- `queryImpact`: `none|tags|all`.

## Poza zakresem

- Rysowanie siatki (E09).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/library/query`
- `src/library/createLibrary.ts`
- `src/library/metadata/queryImpact.ts`

Testy do przeniesienia:
- `src/library/query/__tests__`
- `src/library/__tests__`

Dokumenty i reguły:
- `docs/subsystems/library-query-and-gallery.md`
- `.claude/rules/gallery-query.md`
- `docs/adr/0005-binary-id-ranges-and-anchored-refresh.md`
- `docs/adr/0007-session-timeline-sections.md`
- `docs/adr/0027-unfiltered-sections-unavailable-instead-of-stale.md`
- `docs/adr/0029-range-selection-keeps-its-session-alive.md`

## Kryteria akceptacji

- Przypadki z testów TS przeniesione; lista pominiętych z powodem w notatkach.
- Testy na deterministycznym wykonawcy: odpowiedzi spóźnione, zamienione kolejnością i `stale` w trakcie ładowania strony.
- `total` jest liczbą wyników; żadna struktura nie rośnie liniowo z `total`.
- Strony przechowują `AssetSummary` dokładnie tak, jak przyszły (bez dorabiania pól szczegółów).
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E07 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`

## Scenariusze porównania

- brak (epik bez widocznego odpowiednika w Tauri albo bez UI)

## Ryzyka

- Indeksowanie tablicy cache jak globalnej listy wyników.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
