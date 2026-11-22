# E16: Lightbox: UI, przeglądarka obrazów, zaślepki GIF i wideo

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E09, E13, E15.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Pełny lightbox dla obrazów; GIF i wideo jako zaślepki z działającym panelem bocznym (D8).

## Zakres

- Modal, scena mediów, pasek narzędzi, szyna akcji (ulubione, reset zoomu, pełny ekran, usuń), panele tagów, informacji i grupy mediów; zwijanie panelu w wąskim oknie; wygaszanie kontrolek.
- Przeglądarka obrazów: dekodowanie poza wątkiem UI, zoom 1–12×, kółko zakotwiczone na wskaźniku, dwuklik, przeciąganie z ograniczeniem, klawisze ←/→, `f`, `0`, `+`/`=`/`-`; pełny ekran przez okno.
- Potwierdzenie usunięcia z wpisanym słowem (`lightbox.deleteConfirm.confirmWord` nigdy nie jest tłumaczone maszynowo).
- GIF: statyczna pierwsza klatka + chip „GIF”. Wideo: panel-zaślepka z miniaturą jako plakatem, etykietą „Video” i metadanymi; bez odtwarzania. Nowe klucze i18n zaślepek we wszystkich 11 plikach.

## Poza zakresem

- Animacja GIF (E20).
- Natywne wideo przez libmpv (E21).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/lightbox`
- `src/app/LightboxBoundary.tsx`

Testy do przeniesienia:
- `src/features/lightbox/__tests__`

Dokumenty i reguły:
- `docs/subsystems/lightbox.md`
- `.claude/rules/lightbox-video.md`

## Kryteria akceptacji

- Testy pan/zoom (`lightboxPanZoomService`, `createImageControls`) przeniesione.
- Klawisze i fokus jak w Solid; Escape zamyka tylko górną warstwę.
- Parytet `lightbox-image`, `lightbox-image-zoomed`, `lightbox-delete-confirm`; `lightbox-gif` i `lightbox-video` porównywane poza sceną mediów.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E16 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`
- `bun run locale:check`

## Scenariusze porównania

- `lightbox-image`: Lightbox z obrazem, panel boczny otwarty.
- `lightbox-image-zoomed`: Ten sam obraz po zoomie i przesunięciu.
- `lightbox-gif`: Lightbox z GIF-em (GPUI: zaślepka, D8).
- `lightbox-video`: Lightbox z wideo (GPUI: zaślepka, D8).
- `lightbox-delete-confirm`: Dialog potwierdzenia usunięcia z wpisanym słowem.

## Ryzyka

- Bardzo duże obrazy: limit pamięci dekodowania.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E04

- Z epiku E04: klawisze lightboxa `f`, `0`, `+`, `=`, `-` wpisują znaki: wiąż je przez `text_input::outside_text_input("…")`, inaczej nie da się ich wpisać w polu tagów ani klucza grupy. Strzałki ←/→ wiązane w kontekście lightboxa nie odpalą się z fokusem w polu, bo pole wiąże je głębiej (kursor), jak w Tauri.
- Z epiku E04: słowo potwierdzenia usunięcia i klucz grupy to zwykłe `TextInput`; kolejność w grupie: `set_accept(Some(f))` z odpowiednikiem `parseMediaGroupOrder` odrzuca wpis i wklejenie, po których wartość nie byłaby poprawna (zachowanie `onInput`/`onPaste` z `MediaGroupSetter.tsx`), `set_text` nie jest filtrowane. Porównanie słowa (trim, NFC, małe litery) należy do modelu tego epiku, pole go nie normalizuje. `disabled={isSubmitting}`: `set_disabled` zdejmuje fokus z okna, więc po zakończeniu trzeba go przywrócić (w Tauri robi to `restoreFocusId`).

### 2026-11-21 — implementer E05

- Z epiku E05: modal lightboxa to `ui::modal::ModalState::with_motion(ModalMotion::LIGHTBOX, cx)` (75/50 ms zamiast 150/100) z elementem `Modal`; Escape i tło emitują `ModalEvent::Dismiss`, właściciel zamyka. Pełnoekranowy modal lightboxa ma własny układ, więc może potrzebować wariantu `Modal` bez powierzchni dialogu (dziś `Modal` zawsze rysuje `modal_surface` o szerokości z `ModalSize`). Przywrócenie fokusu na kafel: `ModalState::set_restore_focus`.

### 2026-11-22 — implementer E14

- Z epiku E14: `views::tag_editor::TagEditor` ma pierwszego właściciela (`views::bulk_dock::BulkDock`): wzorzec to znacznik propsów (przebudowa `TagEditorProps` tylko po zmianie tagów, stanu ładowania, blokady albo języka), szkic pchany do edytora tylko wtedy, gdy zmienił go właściciel, i fokus po zapisie klatkę później. Pole edytora dziedziczy teraz krój otoczenia (`font: inherit` w Tauri), monospace mają tylko chipy. `Modal` ma `height(px)` (stała wysokość dialogu, `h-[90vh]`), `Alert` przyjmuje dzieci (przycisk Retry) i `debug_selector`, `Button` z `size` i `height` naraz ma szerokość `size` i wysokość `height`. Element `Modal` zamkniętego dialogu jest pustym `div`: w kolumnie flex z `gap` trzeba go trzymać w pudełku `absolute().size_0()`, inaczej zostawia odstęp.
- Z epiku E14: miniatury poza siatką galerii (plansza kolejności) zgłasza się przez `Thumbnails::sync_board(Vec<TileRequest>)`; to jeden dodatkowy zestaw kafli, więc lightbox potrzebuje własnego albo uogólnienia tego kanału.

### 2026-11-22 — poprawki E15 (audyt orkiestratora)

- Z epiku E15: model lightboxa jest gotowy w `media_tagger_client::lightbox` i osadzony w `AppRuntime` (ADR 0045, `docs/subsystems/lightbox.md`, sekcja „GPUI port: the lightbox model”). Polecenia wysyłaj przez `RuntimeInput::lightbox(LightboxCommand::…)` (`Select { asset, index }`, `Close`, `Previous`, `Next`, `SaveTags`, `RetryTags`, `RetryDetails`, `RetryNavigation`, `ToggleFavorite`, `SetMediaGroupKeyDraft`, `SetMediaGroupOrderDraft`, `SaveMediaGroup`, `DeleteSelected { request }`), stan czytaj przez `AppRuntime::lightbox()` (`LightboxView`: `selected`, `is_open`, `selected_index`, `navigation_status`, `tag_editor`, `tag_saving`/`tag_failed`, `tag_details_loading`/`tag_details_failed`/`tag_editing_disabled`, `asset_details_failed`, `favorite_pending`/`favorite_failed`, `group_pending`/`group_failed`, `media_group_key_editor`, `media_group_order_editor`, `media_group_valid`). Widok nie potrzebuje własnych biletów; gdyby jednak wołał bibliotekę albo zapytanie z biletem, zakres 2^60..2^61 (`LIGHTBOX_TICKET_BASE`..`BULK_TICKET_BASE`) jest zajęty przez model lightboxa, a 2^61..2^62 przez model bulk, więc bilety widoku zostają poniżej 2^60.
- Z epiku E15: widok jest właścicielem czterech maszyn stanu i tworzy je na nowo przy każdym otwarciu lightboxa: `lightbox::Sidebar`, `lightbox::Activity`, `lightbox::GroupClipboard` i `lightbox::DeleteConfirm`. Nie mają timerów: każda metoda dostaje `now: Duration` (czas od dowolnego stałego punktu, ten sam zegar dla wszystkich). Przerysowania planuj z `Sidebar::deadline(now)`, `Activity::deadline(now)` i `GroupClipboard::deadline(now)`: po każdej klatce zapytaj wszystkie trzy z czasem tej klatki i ustaw jeden timer na najwcześniejszy wynik. Zwracany moment jest zawsze późniejszy niż `now`, a po jego upływie metoda zwraca `None`, więc ponowne uzbrajanie timera z `deadline` nie zapętla się; `None` oznacza brak timera. `DeleteConfirm` nie ma terminu. Powiązania: `DeleteConfirm::sync(selected_id)` po każdej zmianie wyboru, `Sidebar::set_locked(confirm.locks_sidebar(), now)`, `Sidebar::set_narrow(is_narrow(szerokość), now)`, `GroupClipboard::sync(klucz, id)`, a zapis do schowka to `begin_copy` → zapis → `finish_copy`.
- Z epiku E15: nie wysyłaj `RuntimeInput` przy każdym ruchu wskaźnika. `Activity::pointer_moved(x, y, now)` (oraz `pointer_down`, `input`, `reveal`) zostaje w widoku; do runtime idą tylko polecenia `LightboxCommand`.
- Z epiku E15: usuwanie. `DeleteConfirm::submit(word)` daje `request`, który idzie w `LightboxCommand::DeleteSelected { request }`; odpowiedź to `LightboxEvent::DeleteSettled { request, result }` z `AppRuntime::take_lightbox_events()`. Encja GPUI `Runtime` nie przekazuje jeszcze tych zdarzeń: dodaj `Runtime::take_lightbox_events` na wzór `Runtime::take_bulk_events` (`src-gpui/crates/media_tagger_gpui/src/runtime.rs`) i odbieraj je w widoku po każdym powiadomieniu encji. Bez tego `DeleteConfirm` zostaje w stanie „deleting” (zamknięcie, Escape, tło i nawigacja zablokowane), a zdarzenia gromadzą się w modelu. Każde `DeleteSelected` dostaje dokładnie jedną odpowiedź, także po `LibraryInput::Dispose` (wtedy `Err(MutationError::Refused(TagStateError::Unavailable))`). Wynik przekaż do `DeleteConfirm::settled(request, &result)` i do `DeleteNotice::of(&result)` (brakujące źródło, oczekujące sprzątanie); tekst odmowy daje `DeleteConfirm::error_text(&i18n)`. Lightbox zamyka się sam, gdy biblioteka opublikuje usunięcie (`LightboxView::is_open`).
- Z epiku E15: przypadki Solid zostawione dla widoku (lista z wiersza `tests/lightbox_ported.rs` w `docs/development/testing.md`, sekcja „GPUI workspace”). `LightboxModal.test.tsx`: wszystkie przypadki poza potwierdzeniem usunięcia i kopiowaniem nazwy grupy, czyli hierarchia, animacja otwarcia i wyjścia, modalność, wąska szuflada, zagnieżdżona nakładka, pole i podpowiedzi edytora tagów, kontrolka Retry szczegółów, błąd mediów, węzeł GIF i skróty; do tego połowy widokowe przypadków przeniesionych tylko w części modelowej (fokus, panel Information), oznaczone w komentarzach testów. `MediaGroupSetter.test.tsx`: pole tekstowe z podpowiedzią numeryczną oraz przycisk Apply wyłączony przez prop `pending` (stan oczekiwania za nim pokrywa przypadek `createLightbox` o powtarzanych poleceniach metadanych). W całości `keyboard.test.tsx` i `imageControls.test.tsx`. Z `presentation.test.ts` została połowa `defaultPrevented` przypadku „reveals without consuming” (model nie przyjmuje zdarzenia widoku, więc to widok, wołając `Activity::input` albo `Activity::pointer_down`, nie może skonsumować zdarzenia).
- Z epiku E15: test usuwania na prawdziwym rdzeniu podczas skanu ma gotowy szew `TempCore::hold_scan_lock()` (tylko cecha `test-support`); wzorzec jest w `src-gpui/crates/media_tagger_client/tests/lightbox_core.rs`.
