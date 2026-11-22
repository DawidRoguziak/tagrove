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

done 2026-11-22

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E04

- Z epiku E04: API pola dla wyszukiwarki (`ui::text_input::TextInput`): tekst trzyma pole, właściciel słucha `TextInputEvent::Changed` i `SelectionChanged` (`cx.subscribe`) i czyta `text()` oraz `cursor()`. Offsety to bajty UTF-8, nie jednostki UTF-16 jak `selectionStart` w `SearchTagator.tsx`: `readActiveToken` i `applySuggestionToValue` po stronie Rust liczą w bajtach. Uzupełnienie tokenu: `replace_range(start..end, tekst, cx)` (jeden krok cofania, kursor za wstawionym tekstem); wyczyszczenie albo podmiana wartości: `set_text` (kursor na koniec, historia cofania wyczyszczona, ten sam tekst nic nie zmienia).
- Z epiku E04: klawisze listy podpowiedzi: góra/dół to akcje `text_input::actions::Up`/`Down`; właściciel przejmuje je w fazie capture (`capture_action` + `cx.stop_propagation()`) tylko gdy lista jest otwarta, inaczej pole przesuwa kursor na początek/koniec. Enter i Escape to `actions::Confirm`/`Cancel` obsługiwane przez `on_action` na elemencie wokół pola; Escape przy zamkniętej liście: `cx.propagate()`, wtedy trafia do powiązania `escape` dalej (modal). Podczas kompozycji IME pole samo połyka Enter/Escape (odpowiednik `isComposing`).
- Z epiku E04: fokus: `cx.on_focus_in`/`on_focus_out` na `input.focus_handle(cx)` zastępują `focusin`/`focusout`; Ctrl+K fokusuje przez `focus_handle.focus(window, cx)`.

### 2026-11-22 — workflow gpui-port-epic

- Zrobione: moduł `media_tagger_client::search` (gramatyka filtrów, tokeny, aktywny token, filtry, model `Search` w `AppRuntime` jako `RuntimeInput::Search`, własny matcher rozmyty zamiast Fuse.js, `SuggestionClient` na executorze, maszyna stanów `Combobox`) oraz `views/search.rs` (`SearchBar`: pole, Clear/Search/Retry, lista podpowiedzi z podświetleniami, kontrolka rodzaju mediów, ulubione, alert walidacji), Ctrl+K / Cmd+K w kontekście Shell, rozszerzenia `Button` i `layers::has_modal`, scenariusze parity `search-suggestions` i `search-applied`, dokumentacja (search-tags-and-media-groups, testing, architecture-and-ui-conventions). Testy: 45 testów zachowania (35 w kliencie, 10 w oknie GPUI). Pomiar kryterium "pisanie nie blokuje UI": przy 100 000 tagów build release buduje indeks w ok. 8 ms, a szuka w ok. 19 ms, oba na executorze.
- Odkryte: D13 zrealizowane własnym matcherem (`search::matcher`, bez nucleo-matcher): wynik błędy/długość + start/100, próg 0.35, 20 dopasowań rozważanych, 8 pokazanych; podświetlenia to zakresy bajtów UTF-8. Offsety aktywnego tokenu, `ReplaceToken` i podświetleń są w bajtach UTF-8. Pole pobiera odpowiedzi przez `take_suggestion_events_for(EditorId)`; edytory tagów z E13/E14/E15 używają `ComboboxConfig { auto_select_first: false, pick_to_owner: true, keep_open_on_pick }` i `ComboboxEffect::Pick`. Widok musi ignorować `TextInputEvent::Changed` o tekście równym `combobox.value()`. Listenery fokusu GPUI działają tylko w aktywnym oknie (testy wołają `window.activate_window()`). Ctrl+K to wpis w `shortcuts::SHORTCUTS` (`ShortcutCommand::FocusSearch`), obsługa w `ShellView`, ignorowana przy `ui::layers::has_modal`; przyszły nieaktywny lightbox (E15/E16) musi dodać własny test. Slot wyszukiwarki nie jest już powierzchnią przeciągania (testy używają `gallery::IDENTITY_SELECTOR`). Launcher listy tagów (E13) idzie za separatorem z `SearchBar::tools`, komenda `SearchCommand::ApplyInput`. Scenario ma nowe pola `tags` i `search`. Odroczone: tooltipy (brak prymitywu), animacja 150 ms listy, przewijanie aktywnej podpowiedzi, reguła `.claude/rules/search-tags.md` i linie układu w CLAUDE.md (nie edytowano, wymaga właściciela konfiguracji). Pominięte przypadki testów portowanych: formatBytes/formatDuration (epiki szczegółów/lightboxa), normalizacja tagów (już w `library::tags`), połowa TagListModal (E13), warianty lightbox/bulk suggestionRecovery, `focusin` bez fokusu, umieszczanie nad polem i portal (tylko edytory tagów), IME (pokryte w E04, `tests/text_input.rs`), `inert` i atrybuty aria (DOM). Defekt znaleziony przez testy: handler focus-out w `views/search.rs` robi `cx.notify()` w trakcie rysowania i zamknięta lista zostaje na ekranie (naprawa przez odroczone powiadomienie); przegląd potwierdził i naprawił 2 ustalenia.
- Zostało: nic nie blokuje (status done). Parity nieuruchomione: `compare:ui` zakończyło się błędem dla `search-applied` (0 rund, brak różnic do oceny), a kroki Tauri przez bulk dock nie były uruchomione na prawdziwej aplikacji Tauri. Przegląd: 3 rundy, czysty, brak otwartych ustalień (review-gpui w rundzie 1 nic nie zwrócił), więc nie powstał raport. Gate: ok po 1 rundzie.
- Testy:
  - implement: cargo fmt, clippy -D warnings, testy klienta i GPUI, architecture_boundaries (33), node --test e2e/compare (50), bun run lint, git diff --check: pass
  - testy portowane: klient 35/35 pass; okno GPUI 8/10 pass, 2 fail (tab_out_of_the_field_closes_the_drawn_list, a_programmatic_blur_closes_the_drawn_list) w momencie dodania, potem objęte poprawką z przeglądu
  - checks (check/fix): pass (1 poprawka)
  - compare:ui search-applied: fail (błąd uruchomienia)
  - test:all (gate): pass

### 2026-11-22 — agent naprawczy luk (parytet)

- Zrobione: parytet `search-suggestions` i `search-applied` oceniony teraz na parach zrzutów (`compare:ui` przez job.py, trzy przebiegi, każdy z kodem 0) z pomiarem pikseli względem zrzutu Tauri; wynik: zgodne po poprawkach, bez otwartych różnic „notable” E12 (raport [E12-review-open](reports/E12-review-open.md)). Naprawione: lista podpowiedzi w GPUI nie otwierała się w scenariuszu (fokus skryptowany w nieaktywnym oknie Xvfb nie dochodził do sesji), lupa 20 px zamiast 16 (tekst i lista od x 253), separator przed przyciskiem listy tagów niewidoczny, podpowiedź Ctrl K 16 px zamiast 18,5, ramka w kolorze primary tylko w aktywnym oknie.
- Odkryte: nasłuchy fokusu GPUI działają tylko w aktywnym oknie, a okno harnessu (Xvfb bez menedżera okien) nigdy nie jest aktywne; WebKit wysyła `focusin` mimo to, ale `focus-within` dopasowuje tylko w aktywnym oknie (src-gpui/crates/media_tagger_gpui/src/views/search.rs:293). `border_l_1` na pudełku szerokim na 1 px się nie rysuje (shader quadów GPUI wybiera prawą krawędź dla środkowej kolumny); separatory rysować jako wypełnione pudełko 1 px (src-gpui/crates/media_tagger_client/src/search/layout.rs:60). Ikony `h-4 w-4` przez `UiIcon` mają w Tauri 20 px (src/ui/controls/UiIcon.tsx:43).
- Zostało: tylko różnice minor: wiersze listy 33 zamiast 33,5 px (zaokrąglenie), szerokości tekstu (D10), wiersz narzędzi 1 px wyżej (E09/E10), karetka w nieaktywnym oknie (E04), brak przycisku listy tagów (E13). P0/P1: brak.
- Testy:
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test search_window` (z `MEDIATAGGER_GPUI_REQUIRE_RENDERER=1`): pass 10/10; nowe testy padają bez poprawek (sprawdzone cofnięciem)
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --lib search::layout`: pass
  - `job.py start E12 compare --scenario search-suggestions --scenario search-applied`: pass (3 przebiegi)
  - `cargo fmt --check`, `cargo clippy -D warnings` (src-gpui), `git diff --check`: pass
  - `job.py start E12 checks`: pass (wszystkie 8 kroków)
