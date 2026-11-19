# E13: Modal listy tagów i wspólny TagEditor

Rozmiar: M. Poziom: standardowy. Zależy od: E12.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Lista wszystkich tagów z doładowywaniem i edytor tagów używany przez lightbox i dok.

## Zakres

- Modal listy tagów: strony z `list_tags`, doładowywanie przy przewijaniu, wielokrotny wybór chipów (Shift+Enter, Shift+Spacja), zastosowanie jako wyszukiwanie.
- `TagEditor`: chipy + pole bez ramki w jednym polu, podpowiedzi z E12.

## Poza zakresem

- brak

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/search/tag-list`
- `src/features/tagging/TagEditor.tsx`
- `src/ui/dom/createVirtualList.ts`

Testy do przeniesienia:
- `src/features/search/tag-list/__tests__`
- `src/features/tagging/__tests__`

Dokumenty i reguły:
- `docs/subsystems/search-tags-and-media-groups.md`

## Kryteria akceptacji

- Testy `createTagList` i `tagListSelectionService` przeniesione.
- Fokus wraca do wyzwalacza po zamknięciu modala.
- Parytet `tag-list-modal`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E13 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `tag-list-modal`: Otwarty modal listy tagów, dwa chipy wybrane.

## Ryzyka

- brak

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
