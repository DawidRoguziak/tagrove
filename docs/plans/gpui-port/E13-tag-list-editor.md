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

done 2026-11-22

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E04

- Z epiku E04: pole bez ramki w edytorze tagów to `TextInput` z `set_chrome(FieldChrome::Bare, cx)` (bez tła i cieni, padding poziomy 4 px jak `px-1`) i `set_height(Some(32.0), cx)` (`h-8`); pierścień fokusu rysuje kontener edytora (`.tag-editor:has(input:focus)` w `src/styles.css`), stan fokusu z `cx.on_focus_in`/`on_focus_out` na uchwycie pola. Wyłączenie: `set_disabled(true, window, cx)`.
- Z epiku E04: Shift+Enter i Shift+Spacja na chipach listy tagów: spacja wpisuje znak, więc jej skrót wiąż przez `text_input::outside_text_input(kontekst)`; Shift+Enter nie jest powiązany w polu.

### 2026-11-22 — workflow gpui-port-epic

- Zrobione: Modal listy tagów i wspólny TagEditor w GPUI (bez zmian w core, src-tauri, Cargo.lock i zachowaniu Tauri). Klient: `search::tag_list` (`selection.rs`, `layout.rs`, model `TagList`: strony po 100, paginacja automatyczna i przy przewijaniu 120 px, timer pojedynczego kliknięcia 220 ms, Apply), `tagging.rs` (logika TagEditora bez GUI), scenariusz `tag-list-modal`. Adapter: `views/tag_list.rs` (launcher po separatorze w wierszu narzędzi, modal z filtrem, podsumowaniem, chipami i Cancel/Apply), `views/tag_editor.rs` (encja `TagEditor`), wspólne `suggestion_list_body` w widoku wyszukiwania. Docs: sekcja "GPUI port: tag list and tag editor" w `docs/subsystems/search-tags-and-media-groups.md` oraz testing.md, system-overview.md i architecture-and-ui-conventions.md. Harness: `e2e/compare/scenarios/tag-list-modal.js` i wpis w `e2e/compare/guards.test.js`. Testy: dodano 12 testów (`tests/tag_list_core.rs` 5, `tag_list_window.rs` +5, `tag_editor_window.rs` +2), 17 przypadków TS przeniesionych. Pętla przeglądu: 6 rund, 8 potwierdzonych znalezisk naprawionych.
- Odkryte: Model `TagList` żyje w `AppRuntime` (`RuntimeInput::TagList`), widok galerii wysyła `Close` przy zwolnieniu. Apply idzie przez `SearchCommand::ApplyInput`, a `ApplySettled { ok }` wraca, gdy zapytanie przestanie być pending (różnica od Solid: nowsze odświeżenie jest odczekiwane). Restart listy tylko przy zmianie przyciętego filtra lub `Arc` słownika. Metryki przewijania płyną z widoku jako `TagListInput::ListMetrics`; zmiana `scroll_top` liczy się jak przewinięcie użytkownika. `compare_tags` przybliża `Intl.Collator` (base sensitivity) tabelą, bez nowej zależności. API TagEditora dla E14 i E16: `views::tag_editor::TagEditor::new(runtime, TagEditorProps, window, cx)`, `set_props`, `set_draft`, `focus`, `draft`; zdarzenia `DraftChanged`, `AddTag`, `RemoveTag`; `remove_label: None` oznacza chipy tylko do dodawania; teksty odświeża właściciel przy zmianie języka; selektory `bulk-tag-list`/`lightbox-tag-list`, `tag-editor-chip-N`, `tag-editor-remove-N`, `tag-editor-input`, `tag-editor-suggestions`. Lista podpowiedzi edytora to `deferred(anchored())` z `tagging::list_placement`; pole wyszukiwania zostaje przy liście inline. Modal jest dzieckiem `SearchBar`, więc akcje przewijania i zaznacz-wszystko w galerii wracają wcześnie przy `layers::has_modal(cx)`. Shift+Enter i Shift+Spacja na chipach wiązane lokalnie (`views::tag_list::key_bindings`, kontekst `TagListChip`), nie w `shortcuts::SHORTCUTS`. Scenariusz po stronie GPUI używa podwójnych kliknięć (bez timera). Testy okien dialogów muszą dostarczać klatki (`window.simulate_next_frame(cx)`); podwójny klik to MouseDown/MouseUp z click_count 2. Odroczone: tooltipy `title` na chipach czekają na prymityw tooltipa; filtr używa wspólnej ramki Boxed (wysokość 42 px, bez `py-2.5`, `shadow-surface` i obramowania fokusu); TagEditor nie ma jeszcze konsumenta (teksty i18n, `tagInvalidCharacters`, stan zapisu należą do E14/E16). Pominięte przypadki testowe: dwa testy memo TagListChipList (mechanika Solid), "follows a language change" (nazwa dostępna nie czytelna z TestAppContext), "contains chips followed by the input" (klasy DOM, pokryte testami układu), "does not prepare the tag vocabulary while closed" (GPUI ładuje słownik przy starcie; pokryte `the_known_tags_are_prepared_only_while_open`), `createVirtualList.test.ts` (lista bez wirtualizacji). Różnice parytetu zostawione jako minor (tag-list-modal): brak tooltipu "cat (inactive)" przy chipie, etykiety chipów cięższe o ok. 2 px węższe chipy, placeholder filtra ok. 1 px większy i pole ok. 2 px wyższe. Wydajność: render modalu buduje i układa każdy załadowany chip przy każdej klatce przewijania (`src-gpui/crates/media_tagger_gpui/src/views/tag_list.rs`, render chipów); Solid też nie wirtualizuje, ale przeglądarka nie układa listy od nowa. Do pomiaru, patrz raport poniżej.
- Zostało: Nie zablokowane. Brak otwartych awarii. Trzy niepewne znaleziska wydajnościowe P3 (r2-perf-1, r4-gpui-3, r6-gpui-1, ten sam problem) czekają na pomiar czasu klatki w buildzie release przy ok. 1000, 5000 i 20000 chipów; raport: `docs/plans/gpui-port/reports/E13-review-open.md`. Nie uruchomiono `compare:ui tag-list-modal` w kroku implementacji (parytet zrobiony w workflow, 3 rundy, minor powyżej). Brak sprawdzonych na realnym wyświetlaczu zachowań poza porównaniem. Brama: przeszła w 1 rundzie.
- Testy:
  - implement: fmt --check, clippy -D warnings, media_tagger_client, media_tagger_gpui, architecture_boundaries (33), node --test e2e/compare (50), gpui:lock-parity, bun run lint, git diff --check: pass
  - testy portowane: tag_list_core 5, tag_list_window 10, tag_editor_window 9: pass
  - checks (job.py): pass, 0 poprawek
  - parity compare:ui tag-list-modal: 3 rundy, brak otwartych, 3 minor
  - review: 6 rund, clean, 3 niepewne P3
  - test:all (brama): pass, 1 runda

### 2026-11-22 — gap fixer E13 (r2-perf-1, r4-gpui-3, r6-gpui-1)

- Zrobione: Zmierzono czas klatki listy tagów (build release, platforma testowa GPUI): przed poprawką przewijanie kółkiem 11 / 57 / 406 ms średnio przy 1000 / 5000 / 20000 załadowanych tagach (hover 10 / 48 / 346 ms, samo przerysowanie 6 / 28 / 203 ms), więc znalezisko potwierdzone jako P2 (powyżej 16 ms przy 5000 tagach, czyli 50 doładowaniach). Naprawione: widok pakuje przyciski w wiersze jak `flex flex-wrap gap-1.5` (`media_tagger_client::search::tag_list::rows::ChipRows`, szerokości tekstu mierzone raz na stronę przez `text_system().shape_line`, pakowanie cache'owane per wersja wierszy i szerokość listy) i rysuje tylko wiersze w widocznej części z 3 wierszami zapasu z każdej strony oraz wiersz przycisku z fokusem i jego sąsiadów; niewyrysowane wiersze zajmują miejsce jako margines, więc wysokość treści i paginacja 120 px się nie zmieniają (`src-gpui/crates/media_tagger_gpui/src/views/tag_list.rs:581` `pack_rows`, `:613` `focused_index`, `:850` wybór wierszy, `:670` ponowne pakowanie po zmianie szerokości). Odsłanianie przycisku z fokusem liczone z wierszy zamiast z granic po layoucie (`reveal_chip` usunięte). Po poprawce 1,5–2 ms na klatkę przy każdej liczbie tagów, 62 przyciski na klatkę. Tabela i opis: `docs/plans/gpui-port/reports/E13-review-open.md`; docs: `docs/subsystems/search-tags-and-media-groups.md` ("Drawn rows"), `docs/development/testing.md`.
- Odkryte: GPUI buduje i układa od nowa każdy rysowany element w każdej klatce, a widoki nie są cache'owane, więc osobna encja dla listy nie pomogłaby (przewinięcie i hover i tak ją unieważniają). Klatka przed pierwszym layoutem listy nie zna szerokości: rysuje wszystkie przyciski jednym flex-wrap i prosi o kolejną klatkę (`tag_list.rs:670`). Pamięć: ok. 330 B sterty na załadowany tag (licznik alokacji, jednorazowo, nazwy po 16 B), czyli ok. 6,5 MB przy 20 000 i ok. 33 MB przy 100 000 tagów; nie jest to problem, model zostaje bez zmian. `merge_tag_pages` przy każdej stronie kopiuje całą pokazaną listę (koszt na doładowanie, nie na klatkę).
- Zostało: nic z tych trzech znalezisk; brak otwartych P0 i P1 dla E13. Nie sprawdzone na prawdziwym wyświetlaczu (GPU, prawdziwe kształtowanie tekstu Inter) ani w `compare:ui tag-list-modal` (scenariusz ma 3 tagi, mieszczą się w jednym wierszu).
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked --release -p media_tagger_gpui --test tag_list_window frame_time -- --ignored --nocapture` → pomiar powyżej; `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test tag_list_window` → pass (15, w tym nowe `a_frame_draws_the_rows_in_view_whatever_the_number_of_loaded_tags`, które bez poprawki pada, i `the_drawn_rows_are_the_rows_of_flex_wrap`); `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --lib tag_list` → pass (7 nowych testów `rows`); `job.py start E13 checks` → pass (fmt, clippy, architecture_boundaries, lock-parity, testy obu crate'ów).
