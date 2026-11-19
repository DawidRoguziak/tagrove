# E12: Wyszukiwarka, gramatyka filtrów i podpowiedzi

Rozmiar: L. Poziom: standardowy. Zależy od: E06, E08.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Pasek wyszukiwania z tą samą gramatyką i podpowiedziami tagów liczonymi poza wątkiem UI.

## Zakres

- Gramatyka w kliencie: tagi włączone/wyłączone, operatory `tags`, `tags:N`, `gN:wartość`; stan roboczy i zastosowany; filtr rodzaju (all/image/gif/video) i tylko-ulubione.
- Podpowiedzi: dopasowanie rozmyte w zadaniu tła zamiast workera Fuse.js (D13), zakresy podświetlenia, anulowanie nieaktualnych zapytań.
- UI: chipy + pole, lista podpowiedzi (góra/dół, Enter, Escape), Ctrl+K, kontrolki filtrów.

## Poza zakresem

- Modal listy tagów (E13).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/search/SearchTagator.tsx`
- `src/features/search/SearchTagatorWrapper.tsx`
- `src/features/search/createSearch.ts`
- `src/features/search/filterService.ts`
- `src/features/search/suggestions`
- `src/utils/media.ts`
- `src/utils/searchTokens.ts`

Testy do przeniesienia:
- `src/features/search/__tests__`
- `src/features/search/suggestions/__tests__`
- `src/utils/__tests__`

Dokumenty i reguły:
- `docs/subsystems/search-tags-and-media-groups.md`
- `.claude/rules/search-tags.md`

## Kryteria akceptacji

- Testy gramatyki i tokenów przeniesione 1:1.
- Ranking podpowiedzi spełnia oczekiwania produktowe z testów `suggestionService` (np. kolejność dopasowań); różnice opisane w notatkach.
- Wpisywanie nie blokuje UI przy słowniku ≥ 100 000 tagów (pomiar w notatkach).
- Parytet `search-suggestions`, `search-applied`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- nucleo-matcher (tylko jeśli własny matcher nie spełni testów)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E12 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `search-suggestions`: Wpisany prefiks tagu, otwarta lista podpowiedzi z podświetleniem.
- `search-applied`: Zastosowany filtr tagu i rodzaju; licznik wyników.

## Ryzyka

- Ranking inny niż Fuse.js: liczą się oczekiwania z testów, nie identyczne punkty.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
