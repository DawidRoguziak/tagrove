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

### 2026-11-21 — implementer E04

- Z epiku E04: API pola dla wyszukiwarki (`ui::text_input::TextInput`): tekst trzyma pole, właściciel słucha `TextInputEvent::Changed` i `SelectionChanged` (`cx.subscribe`) i czyta `text()` oraz `cursor()`. Offsety to bajty UTF-8, nie jednostki UTF-16 jak `selectionStart` w `SearchTagator.tsx`: `readActiveToken` i `applySuggestionToValue` po stronie Rust liczą w bajtach. Uzupełnienie tokenu: `replace_range(start..end, tekst, cx)` (jeden krok cofania, kursor za wstawionym tekstem); wyczyszczenie albo podmiana wartości: `set_text` (kursor na koniec, historia cofania wyczyszczona, ten sam tekst nic nie zmienia).
- Z epiku E04: klawisze listy podpowiedzi: góra/dół to akcje `text_input::actions::Up`/`Down`; właściciel przejmuje je w fazie capture (`capture_action` + `cx.stop_propagation()`) tylko gdy lista jest otwarta, inaczej pole przesuwa kursor na początek/koniec. Enter i Escape to `actions::Confirm`/`Cancel` obsługiwane przez `on_action` na elemencie wokół pola; Escape przy zamkniętej liście: `cx.propagate()`, wtedy trafia do powiązania `escape` dalej (modal). Podczas kompozycji IME pole samo połyka Enter/Escape (odpowiednik `isComposing`).
- Z epiku E04: fokus: `cx.on_focus_in`/`on_focus_out` na `input.focus_handle(cx)` zastępują `focusin`/`focusout`; Ctrl+K fokusuje przez `focus_handle.focus(window, cx)`.
