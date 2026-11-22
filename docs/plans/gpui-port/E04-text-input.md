# E04: Pole tekstowe (własny element GPUI)

Rozmiar: M. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E03.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Jednowierszowe pole tekstowe, którego GPUI nie ma w zestawie, zbudowane u nas zamiast brać bibliotekę komponentów od osoby trzeciej (D1).

## Zakres

- Model edycji w kliencie (czysty, bez GPUI): bufor, kursor, zaznaczenie, ruch po grafemach i słowach, wstawianie, usuwanie, zaznacz wszystko, stos cofnij/ponów.
- Element GPUI: rysowanie tekstu, kursora i zaznaczenia, przewijanie w poziomie przy przepełnieniu, placeholder, stan wyłączony, obwódka fokusu z tokenów motywu.
- Wejście: klawiatura (strzałki, Home/End, z Shift i Ctrl), mysz (klik, przeciąganie, dwuklik na słowie), schowek (kopiuj, wytnij, wklej), kompozycja IME przez handler wejścia GPUI.
- Kontekst klawiszy: skróty globalne (np. Ctrl+K, klawisze lightboxa) nie odpalają się, gdy pole ma fokus i samo obsługuje klawisz; Enter i Escape trafiają do właściciela pola.
- Punktem wyjścia jest przykład `crates/gpui/examples/input.rs` z przypiętego commita Zed (Apache-2.0): zachować informację o pochodzeniu w nagłówku pliku.
- Miejsca użycia w kolejnych epikach: wyszukiwarka, edytor tagów, klucz grupy, słowo potwierdzenia usunięcia, ścieżki mapowania rootów.

## Poza zakresem

- Pole wielowierszowe, formatowanie i podświetlanie składni.
- Chipy i podpowiedzi (E12, E13).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/search/SearchTagator.tsx`
- `src/features/search/suggestions/inputService.ts`
- `src/features/tagging/TagEditor.tsx`
- `src/features/lightbox/LightboxDeleteConfirmDialog.tsx`
- `src/features/lightbox/MediaGroupSetter.tsx`

Testy do przeniesienia:
- `src/features/search/suggestions/__tests__`
- `src/features/tagging/__tests__`

Dokumenty i reguły:
- `docs/frontend/architecture-and-ui-conventions.md`

## Kryteria akceptacji

- Testy modelu edycji: emoji i sekwencje ZWJ, znaki łączące, CJK, ruch po słowach, zaznaczanie w obie strony, cofnij/ponów, wklejenie tekstu z nową linią (zamiana na spację albo odrzucenie, opisane).
- Testy `#[gpui::test]`: fokus, wpisywanie, skróty schowka, Enter/Escape do właściciela, skrót globalny nie odpala się podczas pisania.
- Kompozycja IME sprawdzona ręcznie albo testem na handlerze wejścia; wynik w notatkach.
- Długi tekst przewija się, a kursor zostaje widoczny.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- unicode-segmentation

## Testy i sprawdzenia

Uruchamiane przez `job.py start E04 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- brak (epik bez widocznego odpowiednika w Tauri albo bez UI)

## Ryzyka

- IME i kompozycja różnią się między X11 i Waylandem.
- Indeksy bajtowe UTF-8 kontra UTF-16 w API wejścia GPUI: błąd daje panikę na granicy znaku.

## Status

done 2026-11-21

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — workflow gpui-port-epic
- Zrobione: pole tekstowe jest gotowe jako model bez GUI `media_tagger_client::text` (`LineEditor`: zaznaczenie jako kotwica i głowa, ruch po graphemach i słowach, wstawianie, usuwanie, wytnij/wklej, `replace_range`, cofnij/ponów, kompozycja IME w UTF-16, opcjonalny filtr) oraz encja GPUI `media_tagger_gpui::ui::text_input::TextInput` (wzorowana na `examples/input.rs` z Zed, nagłówek z pochodzeniem i licencją Apache-2.0). Rysuje tekst, karetkę i zaznaczenie z przycinaniem, przewija poziomo, ma placeholder i stan wyłączony, ramkę i pierścień fokusu z tokenów motywu; mysz (klik, Shift+klik, przeciąganie, podwójny i potrójny klik), schowek i handler wejścia GPUI działają. Wklejone łamania wierszy: końcowe odpadają, pozostałe stają się jedną spacją (`\r\n` liczone raz), tabulator to spacja, inne znaki sterujące odpadają (opis w `text/sanitize.rs`, ADR 0038 i stronie konwencji frontendu). Dodano zależność `unicode-segmentation` 1.13 tylko w crate klienta (zmiana `src-gpui/Cargo.lock` o jedną linię). Dokumentacja: ADR 0038 z wpisem w indeksie, strona konwencji frontendu, testing.md, system-overview.md, setup-and-build.md oraz notatki "Z epiku E04" w E05, E06, E12, E13 i E16. Testy: 22 nowe przypadki (6 przeportowanych z TS: dwa z `inputService`, część pola z `TagEditor`; pozostałe to przypadki ze źródłowych komponentów i nowe testy kryteriów akceptacji: klastry, zaznaczenie w obu kierunkach, wklejanie i cofanie, skróty globalne, długi tekst, IME, schowek i wskaźnik).
- Odkryte: decyzje obowiązujące kolejne epiki (ADR 0038): (1) pole wiąże `media_tagger_client::text::KEYMAP` w kontekście klawiszy `TextInput` przez `ui::init(cx)`; skrót nigdy nie jest wiązany bez kontekstu, a skrót na klawiszu wpisującym znak (litera, cyfra, Spacja, także z Shift) wiąże się z `text_input::outside_text_input(context)`; przechwytywanie klawiszy (interceptor) odrzucono. (2) Klawisze nieużywane przez pole (np. Ctrl+K) nadal działają przy fokusie w polu, jak w Tauri. (3) Enter i Escape to akcje `actions::Confirm` i `Cancel`, obsługuje je właściciel przez `on_action`; `cx.propagate()` przekazuje dalej; w trakcie kompozycji pole je połyka i kończy kompozycję. (4) Strzałki góra/dół to `Up`/`Down`; właściciel z listą podpowiedzi używa `capture_action` + `cx.stop_propagation()`; Tab niepowiązany, fokus jest przystankiem tab. (5) Pole jest właścicielem edytowanego tekstu; właściciele subskrybują `TextInputEvent::Changed` i `SelectionChanged`, piszą przez `set_text` albo `replace_range`. (6) Wszystkie offsety to bajty UTF-8, nie UTF-16 jak w DOM; serwis tokenów E12 musi liczyć w bajtach. (7) Reguła jednej linii dotyczy pisania, wklejania, kompozycji i `set_text`; WebKit zachowuje wklejony tabulator, to pole nie. (8) Grupowanie cofania: ciągłe pisanie to jeden krok, słowo po spacji zaczyna nowy, seria Backspace lub Delete to jeden krok, wklejenie, wycięcie, usunięcie słowa, nadpisanie zaznaczenia i zakończona kompozycja to osobne kroki, ruch karetki zamyka krok, limit 100. (9) Ruch karetki nie kończy kompozycji; każda zmiana tekstu i utrata fokusu ją kończą, zachowując tekst. (10) `TextInput::new(theme, window, cx)` bierze `ThemeName`, jest `set_theme`; globalny motyw z E05 powinien go zastąpić albo wołać `set_theme`. (11) `FieldChrome::Bare` daje pole bez ramki dla edytora tagów, `set_height(Some(px))` stałe wysokości `h-8`/`h-10`; font dziedziczony. (12) `set_accept(fn(&str) -> bool)` odrzuca edycje (filtr cyfr kolejności grupy); `set_text` nie jest filtrowane, kompozycja sprawdzana po zakończeniu. (13) `set_disabled(true, ...)` na polu z fokusem rozmywa okno; powłoka E06 musi odzyskać fokus w `on_focus_lost`. (14) Tabela klawiszy żyje w crate klienta jako napisy GPUI, adapter mapuje je na akcje; `EditText(Edit)` to akcja danych `no_json`. (15) Granice słów wg UAX #29: segment jest słowem, gdy ma literę, cyfrę, podkreślnik albo znak nie-ASCII niebędący spacją. Odroczone: prawdziwy IME na X11/Wayland nieprowadzony (kompozycja tylko testami handlera, żadne pole nie jest jeszcze widoczne w aplikacji); brak migotania karetki (celowo, stabilne zrzuty; do rozważenia w E19); brak wklejania środkowym przyciskiem, autoprzewijania przy przeciąganiu poza polem i zaznaczania całości przy fokusie Tab; Backspace usuwa cały klaster graphemów (WebKit czasem tylko znak łączący, opisane w ADR 0038); dostępność minimalna (rola `TextInput` i `set_label`, bez wartości i placeholdera); brak pełnego tekstu licencji Apache-2.0 w repozytorium dla pliku pochodnego (pytanie pakietowe dla E22 lub użytkownika); `CLAUDE.md` i `.claude/rules/gpui-port.md` nieedytowane (mogłyby wspomnieć `text.rs`/`text/`, `ui/`, cel testowy `text_input` i dwie reguły skrótów). Pominięte przypadki testów TS (zakres E12/E13): `createSuggestions.test.ts` (3 przypadki: obsolete results, retry po błędzie workera, utrzymanie błędu po blur), `SuggestionClient.test.ts` (4, mechanika Web Workera), `suggestionService.test.ts` (6, ranking i gramatyka wyszukiwania), `tokenService.test.ts` (6, parsowanie tokenów; część pola pokrywa przeportowany `inputService`), `TagEditor.test.tsx` (układ chipów i `border-0`, usuwanie chipów, wykluczenia podpowiedzi, stan ładowania i listy opcji). Różnice parity: brak (0 rund, brak otwartych).
- Zostało: nic zablokowanego, brak nieprzechodzących testów, przegląd czysty po 3 rundach (4 potwierdzone, 4 naprawione, 0 otwartych, bez raportu), bramka test:all zaliczona w 1 rundzie. Do ręcznej weryfikacji: prawdziwy IME na wyświetlaczu, gdy pojawi się pierwsze widoczne pole (E05/E06); decyzja o pełnym tekście Apache-2.0 i o uzupełnieniu CLAUDE.md.
- Testy:
  - `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`: pass
  - `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`: pass
  - `cargo test --locked -p media_tagger_client` (295 testów jednostkowych, cel `text_editor` 13/13): pass
  - `cargo test --locked -p media_tagger_gpui` (`text_input` 19/19, `text_input_owner` 9/9): pass
  - `architecture_boundaries` (33): pass
  - `bun run gpui:lock-parity`: pass
  - `port_state.py validate`: pass
  - `bun run test:all` (bramka): pass
  - `bun run quality`, `privacy:check`: nie uruchomiono osobno przez agentów (poza bramką workflow)

### 2026-11-22 — agent naprawczy luk E12

- Z epiku E12: `TextInput` rysuje karetkę także w nieaktywnym oknie; pole Tauri (WebKit) w nieaktywnym oknie karetki nie pokazuje. Widać to w zrzucie porównania `search-suggestions` (okno harnessu nigdy nie jest aktywne). Różnica minor, nienaprawiona (docs/plans/gpui-port/reports/E12-review-open.md).
