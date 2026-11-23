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

done 2026-11-23

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

### 2026-11-22 — implementer E16

- Zrobione: widok lightboxa w adapterze (`views/lightbox.rs`, `views/lightbox/stage.rs`, `views/lightbox/sidebar.rs`), ładowanie i dekodowanie oryginału poza wątkiem UI (`media_tagger_gpui::media::Originals`), a w kliencie: `lightbox/pan_zoom.rs` (port `lightboxPanZoomService.ts`), `lightbox/image_controls.rs` (port `createImageControls.ts` i pełny ekran przez okno), `lightbox/media.rs` (co pokazuje scena, format z treści pliku, budżet dekodowania, pierwsza klatka GIF-a), `lightbox/info.rs` (`formatBytes`, `formatDuration`, podsumowanie szyny, wiersze panelu informacji), `lightbox/layout.rs` (miary) i `lightbox/session.rs` (`Session`: jedna odsłona lightboxa z regułami Escape, tła, klawiszy, fokusu, wygaszania i wysuwania panelu). Powłoka trzyma `LightboxView` przez całe życie okna i otwiera go na `GalleryEvent::OpenAsset`. Klawisze w `KeyContext::Lightbox` (`shortcuts.rs`). Pięć scenariuszy porównania: pliki `e2e/compare/scenarios/lightbox-*.js`, `ScenarioLightbox` w `scenario.rs`, stan startowy przez `Runtime::script_lightbox`. Klucze `lightbox.placeholder.videoUnavailable` i `lightbox.placeholder.gifStill` w 11 plikach. ADR 0046; sekcja „GPUI port: the lightbox view” w `docs/subsystems/lightbox.md`.
- Odkryte: GPUI nie eksportuje crate'a `image`, więc adapter dekoduje tylko przez `gpui::Image::to_image_data`, który trzyma wszystkie klatki animacji i nie skaluje; stąd obcięcie GIF-a do pierwszej klatki w bajtach i odmowa dekodowania ponad budżet zamiast pomniejszania (`lightbox/media.rs`). `window.paint_image` zwraca błąd, gdy GPU nie przyjmie tekstury (`gpui_wgpu/src/wgpu_atlas.rs`), a widok zamienia go w błąd mediów (`Originals::draw_failed`). Testy okna rysują klatkę po `settle`, bo fokus i slajd ruszają klatkę później. Trzy istniejące testy adaptera zakładały, że nikt nie obsługuje `OpenAsset`; teraz zamykają lightbox przed dalszą pracą na siatce (`tests/gallery_grid.rs`, `tests/gallery_selection.rs`), a `tests/scenario.rs` i `e2e/compare/guards.test.js` znają pięć nowych scenariuszy.
- Zostało: przeniesienie zestawu testów Solid (`imageControls.test.tsx`, `keyboard.test.tsx`, przypadki widokowe `LightboxModal.test.tsx` i `MediaGroupSetter.test.tsx`); różnice do domknięcia zapisane w notatkach E19; animacja GIF (E20) i wideo (E21) w notatkach tamtych epików. `LightboxBoundary.tsx` nie ma odpowiednika: adapter nie ładuje widoków leniwie. Dwuklik zostaje bez własnego wiązania, jak w Solid (`double_click_zoom_factor` jest przeniesione i nieużywane).
- Testy: `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings` → pass; `cargo test --manifest-path src-gpui/Cargo.toml --locked --no-fail-fast` → pass; `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries` → pass; `bun run test:compare`, `bun run locale:check`, `bun run gpui:lock-parity`, `bun run lint` → pass; `job.py start E16 compare` dla pięciu scenariuszy → zrzuty obu aplikacji powstały, układ zgodny poza sceną GIF i wideo oraz rasteryzacją fontów.

### 2026-11-22 — poprawki po przeglądzie E16 (runda 1)

- Zrobione: wpis „Zostało” implementera o przeniesieniu testów Solid jest nieaktualny: zestaw jest przeniesiony w `src-gpui/crates/media_tagger_client/tests/lightbox_viewer_ported.rs` (`imageControls.test.tsx` na `ImageControls`, `keyboard.test.tsx` na `Session`; tu są testy pan/zoom z kryterium akceptacji) i w `src-gpui/crates/media_tagger_gpui/tests/lightbox_ported.rs` (połowy widokowe `LightboxModal.test.tsx`, `MediaGroupSetter.test.tsx`, `keyboard.test.tsx`, `imageControls.test.tsx` i `presentation.test.ts`). Oba pliki mają wiersze w `docs/development/testing.md` („GPUI workspace”), a `docs/subsystems/lightbox.md` je wymienia.
- Zrobione: `Originals` (`media_tagger_gpui/src/media.rs`) prowadzi jedno ładowanie naraz. Rozpoczętego dekodowania nie da się przerwać, więc źródło pokazane w trakcie czeka na jego koniec, trwające ładowanie rezygnuje przed odczytem pliku i przed dekodowaniem, gdy jego źródło nie jest już pokazywane, a źródła mijane po drodze (przytrzymana strzałka) nie są w ogóle czytane. `Originals::loads_started` liczy ładowania oddane do wykonawcy tła (dla testów).
- Zrobione: pełny ekran przez okno. GPUI ma tylko `toggle_fullscreen`, a okno odpowiada później; drugie przełączenie przed odpowiedzią cofa pierwsze na X11, ale powtarza je na Wayland (okno zostawało na pełnym ekranie). Widok trzyma `lightbox::WindowFullscreen` przez całe życie okna: okna, które nie odpowiedziało, nie przełącza się drugi raz, a gdy odpowie i potrzebny jest już inny stan, przełącza się je jeszcze raz (także po zamknięciu lightboxa). `Fullscreen` i `Session` nadal zwracają „przełącz okno”; każde takie żądanie widok przepuszcza przez `request_window_fullscreen`.
- Zrobione: `format_bytes` zaokrągla dokładną połówkę w górę także poniżej 10 (jak `toFixed(1)`): 1280 B to „1.3 KB”.
- Odkryte: okno testowe GPUI przełącza pełny ekran synchronicznie i nie zgłasza zmiany, więc wyścigu z odpowiedzią okna nie da się odtworzyć w teście okna; pokrywają go testy jednostkowe `WindowFullscreen` i przypadek „drops a shell fullscreen request…” w `lightbox_viewer_ported.rs`. Nie sprawdzono na prawdziwym kompozytorze Wayland.

### 2026-11-22 — poprawki po przeglądzie E16 (runda 2)

- Zrobione: odczyt, który nie wraca (plik na zamontowanym zasobie sieciowym lub FUSE, który przestał odpowiadać), nie blokuje już kolejnych obrazów (`media_tagger_gpui/src/media.rs`). Odczyt i dekodowanie to teraz dwa kroki ładowania. Na odczyt pokazywanego źródła `Originals` czeka tak długo, jak jest pokazywane (jak webview na swój obraz), a scena rysuje wtedy spinner z opóźnieniem kafelka (`lightbox-media-loading`, `StageContent::Pending`). Odczyt źródła, które nie jest już pokazywane, wstrzymuje następne najwyżej przez `READ_DEADLINE` (10 s): potem jest porzucany (zostaje na swoim wątku, jego bajty przepadają po powrocie), a pokazywane źródło dostaje swoje ładowanie. Porzucone odczyty są liczone (`Originals::abandoned_reads`) i ograniczone przez `reads_allowed`: jeden wątek tła zostaje wolny, najwyżej trzy odczyty naraz (`MAX_READS`); ponad to pokazywane źródło czeka, aż któryś wróci, i wtedy jest ładowane. ADR 0046 opisuje regułę i jej koszt.
- Odkryte: zablokowanego `std::fs::read` nie da się odtworzyć w teście na deterministycznym wykonawcy GPUI (zadanie tła biegnie na wątku testu), więc `Originals::set_reader` pozwala podmienić czytnik; w testach zawieszony odczyt to timer na zegarze testu. Świadomie inaczej niż w propozycji z przeglądu: pokazywane źródło nie dostaje błędu po terminie, bo wolny dysk (rozkręcanie talerzy, wolny udział sieciowy) dawałby fałszywe „nie można wyświetlić”; termin zwalnia tylko kolejkę dla innych źródeł. Na maszynie z dwoma wątkami tła żaden odczyt nie jest porzucany (drugi zawieszony odczyt zatrzymałby cały wykonawca, w tym timery), więc tam lightbox nadal czeka.
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test lightbox_ported` (dwa nowe przypadki: `gives_up_a_read_that_does_not_return_once_another_picture_waits`, `shows_the_next_picture_although_the_read_of_the_one_before_never_returns`).

### 2026-11-22 — poprawki po przeglądzie E16 (runda 3)

- Zrobione: odczyt oryginału (`media_tagger_gpui/src/media.rs`, `read_within` i `read_bounded`) czyta tylko zwykły plik i nigdy więcej niż `MAX_SOURCE_BYTES`, niezależnie od długości, którą plik podaje. Ścieżka jest sprawdzana przed otwarciem (otwarcie potoku czekałoby na piszącego), a otwarty plik jeszcze raz; dowiązanie do zwykłego pliku jest śledzone, jak w protokole zasobów webview, natomiast urządzenie, potok i katalog pod zindeksowaną ścieżką dają błąd mediów. Odczyt kończy się jeden bajt za budżetem, więc plik, który urósł po sprawdzeniu albo podaje długość 0 (`/proc`), też nie przekroczy budżetu. `MAX_SOURCE_BYTES` jest eksportowane z `media_tagger_client::lightbox`; `source_refusal` zostaje w kliencie, ale adapter już go nie woła.
- Zrobione: wygaszone pływające przyciski (ponowne otwarcie panelu, zamknięcie) nie przyjmują kliknięcia wskaźnikiem, jak przy `pointer-events: none` w `LightboxModal.tsx`. Widok zapamiętuje w `pointer_pressed`, czy naciśnięcie zastało kontrolki wygaszone (`pressed_faded`), i `floating_click_counts` odrzuca wtedy kliknięcie myszy; Enter i spacja na przycisku z fokusem działają zawsze.
- Zrobione: akapit o kluczach `lightbox.placeholder.*` w `docs/development/localization.md` ma własny nagłówek („Keys read only by the GPUI interface”) i nie rozdziela już akapitów o słowie potwierdzenia.
- Odkryte: propozycja z przeglądu (`Button::inert` na wygaszonych przyciskach) odbiera przyciskowi także uchwyt fokusu. Przycisk ponownego otwarcia dostaje fokus po zamknięciu panelu z klawiatury; bez uchwytu w klatce GPUI wysyła klawisze od korzenia okna (`focus_node_id_in_rendered_frame` w `window.rs` przypiętego commita), czyli poza kontekstem klawiszy lightboxa, więc po trzech sekundach bezczynności strzałki i Escape przestałyby działać (wniosek z kodu GPUI, nie z testu). Dlatego przyciski zostają w drzewie fokusu, a odrzucane jest samo kliknięcie wskaźnikiem. Flagi nie można liczyć w chwili kliknięcia: naciśnięcie pokazuje kontrolki, zanim przycisk zostanie puszczony.
- Odkryte, poza zakresem tej rundy: GPUI nie zatrzymuje kliknięcia na przycisku (`hit_test` zbiera wszystkie hitboxy pod wskaźnikiem aż do pierwszego `occlude`), więc kliknięcie pływającego przycisku leżącego nad powiększonym obrazem trafia także do obrazu (`image_clicked`). Przy zamknięciu nie ma to skutku; przy ponownym otwarciu panelu obraz powinien powiększyć się o krok (wniosek z kodu, nie odtworzono). Do sprawdzenia w E19.
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --lib media::` (trzy nowe przypadki) i `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test lightbox_ported --test lightbox_window` (nowy przypadek `a_press_on_a_faded_floating_button_shows_it_without_clicking_it`; bez poprawki nie przechodzi).

### 2026-11-23 — poprawki po przeglądzie E16 (runda 1, kolejny przebieg)

- Zrobione: `probe_dimensions` (`media_tagger_client/src/lightbox/media.rs`) czyta rozmiar nagłówka DIB pliku BMP. Dla `BITMAPCOREHEADER` (12 bajtów, OS/2 1.x) boki są 16-bitowe bez znaku (offsety 18 i 20); wcześniej czytane jako 32-bitowe dawały miliony pikseli, więc mały obraz był odrzucany jako za duży i scena pokazywała błąd mediów. Pozostałe nagłówki czytane są jak dotąd (32 bity ze znakiem).
- Zrobione: to, co pływa nad sceną, zatrzymuje naciśnięcie i kliknięcie wskaźnika (`block_mouse_except_scroll` w `views/lightbox/stage.rs`): pokazane przyciski ponownego otwarcia panelu i zamknięcia (`Frame::controls_visible`; wygaszone nie blokują niczego, jak przy `pointer-events: none`), alerty błędu zapisu, komunikat nawigacji i komunikat o szczegółach. Zamyka to wpis „Odkryte, poza zakresem tej rundy” z rundy 3 powyżej: kliknięcie przycisku nad powiększonym obrazem nie powiększa go już o krok i nie zaczyna przeciągania. Kółko nadal trafia do sceny także nad tymi elementami (pokazuje kontrolki i przybliża); w Solid kółko nad pływającymi przyciskami i komunikatem nawigacji nie przybliża, bo nie są potomkami sceny. Różnica zostaje, bo `occlude` odcięłoby też nasłuch aktywności okna dialogowego.
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --lib lightbox::media` (przypadek `headers_declare_their_size` z oboma nagłówkami BMP) i `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test lightbox_ported --test lightbox_window` (dwa nowe przypadki: `a_click_on_a_floating_button_over_the_picture_does_not_zoom_it`, `a_click_on_the_details_retry_over_the_picture_does_not_zoom_it`; bez poprawki oba nie przechodzą).

### 2026-11-23 — workflow gpui-port-epic
- Zrobione: uruchomienie workflow zaczęło się od kroku `review` (pominięto implementację, testy portowane i pętlę sprawdzeń, więc brak ich podsumowań). Pętla przeglądu zakończyła się po 3 rundach bez otwartych uwag: 4 potwierdzone ustalenia, wszystkie 4 poprawione, 0 odrzuconych.
- Odkryte: bramka `test:all` zatrzymała się w `quality` na kroku `audit:rust`: `cargo audit` zgłasza RUSTSEC-2026-0594 dla `cookie 0.18.2` (poprawka w >=0.18.3), porada opublikowana 2026-11-23. Zależność wchodzi tylko przez stos Tauri (wry, tauri-runtime, tauri), nie ma jej w `src-gpui/Cargo.lock`. Epik nie zmienił `src-tauri/`, `package.json` ani skryptu audytu, więc ten sam błąd wystąpiłby na commicie bazowym. Naprawiający sklasyfikował to jako niezwiązane z epikiem i niczego nie zmieniał. Rozwiązanie jest poza zakresem epiku: podniesienie `cookie` do 0.18.3 w `src-tauri/Cargo.lock` (np. `cargo update --manifest-path src-tauri/Cargo.toml -p cookie --precise 0.18.3`, potem `bun run audit:rust` i `bun run gpui:lock-parity`) albo zrecenzowany `--ignore RUSTSEC-2026-0594` w `audit:rust`; czy wry/tauri akceptują 0.18.3, nie sprawdzono. Luka przeglądu: w rundzie 3 `review-gpui` nic nie zwrócił.
- Zostało: epik jest zablokowany, bo `test:all` nie przechodzi z powodu niezwiązanego z epikiem (2 rundy bramki). Kroki po `audit:rust` (gpui:fmt, gpui:clippy, gpui:lock-parity, test:gpui-tools, test:compare, audit:gpui) oraz wszystkie zestawy testów z `test:all` nie zostały uruchomione w tej bramce, więc dla E16 są niezweryfikowane. Porównanie zrzutów (parity) nie było uruchamiane (0 rund): kryterium zgodności wizualnej nie jest ocenione. Otwartych uwag przeglądu nie ma, raport nie powstał. Dziennik bramki: `docs/review/gpui-port/E16/jobs/test-all.log` (katalog ignorowany).
- Testy: sprawdzenia epiku (checks) nie były uruchamiane w tym przebiegu: brak wyniku; `bun run test:all`: niepowodzenie (zatrzymane w `quality` na `audit:rust`, RUSTSEC-2026-0594, żaden test się nie wykonał).

### 2026-11-23 — orkiestrator

- Zrobione: sprostowanie wpisu „workflow gpui-port-epic” powyżej. Sprawdzenia epiku (`job.py start E16 checks`) BYŁY uruchamiane w tym przebiegu, po poprawkach rund 1 i 2 przeglądu, oba razy z wynikiem pozytywnym; `checks.steps.json` ostatniego przebiegu ma kod 0 dla wszystkich dziewięciu komend. Po rundzie 2 zmieniły się tylko dwie strony dokumentacji (`docs/subsystems/library-query-and-gallery.md`, `docs/subsystems/search-tags-and-media-groups.md`), a runda 3 nie dała poprawek.
- Zrobione: epik miał dwa przebiegi workflow. Pierwszy (2026-11-22, od kroku `implement`, przerwany ręcznie w rundzie 4 przeglądu) wykonał implementację, testy, stabilizację, porównanie zrzutów (1 runda, 5 par, 0 różnic) i rundy przeglądu 1–3: 12 potwierdzonych ustaleń, wszystkie poprawione. Drugi (2026-11-23, od kroku `review`) wykonał 3 rundy przeglądu: 4 potwierdzone ustalenia, wszystkie poprawione, i dwie rundy bramki. Zakres przeglądu był pełny w każdej rundzie obu przebiegów (liczba plików zgodna z `file_count`: 64, w ostatniej rundzie 66).
- Odkryte: luka przeglądu „runda 3: `review-gpui` nic nie zwrócił” dotyczy rundy, przed którą zmieniły się tylko dwie strony dokumentacji; ten sam kod `review-gpui` przejrzał w rundzie 2 bez ustaleń. `review-tooling`, który w pierwszym przebiegu nie zwrócił wyniku w rundzie 2, w drugim przebiegu zwrócił wynik w rundach 1 i 2 na pełnym zakresie (bez ustaleń).
- Zostało: porównanie zrzutów pochodzi sprzed poprawek przeglądu, które zmieniły rysowanie sceny (spinner oczekującego odczytu, wygaszone i blokujące wskaźnik pływające przyciski), więc pięć scenariuszy `lightbox-*` trzeba porównać ponownie i obejrzeć przed zamknięciem epiku. Bramka `test:all` nie wykonała żadnego testu (zatrzymanie na `audit:rust`, RUSTSEC-2026-0594); po usunięciu tej przyczyny poza epikiem wznowienie od kroku `gate`. Audyt braków po workflow (leady z notatek soczewek, kryteria akceptacji) nie został jeszcze zrobiony.
- Testy: `job.py start E16 checks` → pass (dwukrotnie w drugim przebiegu); `job.py start E16 test-all` → fail (`audit:rust`, przed jakimkolwiek testem).

### 2026-11-23 — poprawki po audycie orkiestratora (E16)

- Zrobione: kółko nad tym, co pływa obok sceny, nie przybliża już obrazu i nadal liczy się jako aktywność. Pokazane przyciski ponownego otwarcia i zamknięcia, alerty błędu zapisu i komunikat nawigacji mają własny nasłuch kółka (`floating_wheel` w `views/lightbox/stage.rs`), który woła `activity` i zatrzymuje zdarzenie; `occlude` nie było potrzebne. Wygaszone przyciski niczego nie blokują, więc kółko nad nimi trafia do sceny (`pointer-events: none`). Komunikat o szczegółach zostaje bez zmian celowo: w `LightboxMediaStage.tsx` jest potomkiem sceny (`ref={props.controls.viewportRef}`), więc w Solid kółko nad nim też przybliża. To prostuje wpis „runda 1, kolejny przebieg” powyżej, który zaliczał go do rodzeństwa sceny.
- Zrobione: `WindowFullscreen` zapomina żądanie, na które okno nie odpowiedziało w ciągu `WINDOW_ANSWER_TIMEOUT` (3 s; `media_tagger_client/src/lightbox/image_controls.rs`). Wcześniej żądanie nigdy nieprzyznane przez menedżera okien zostawało w `asked` na całe życie okna, a pełny ekran włączony później przez użytkownika był brany za spóźnioną odpowiedź i od razu cofany. `request` i `window_changed` dostają `now`; reguła „nie przełączaj okna, które jeszcze nie odpowiedziało” zostaje w tym czasie bez zmian. Koszt zapisany w ADR 0046 i w „Known limitations”: okno, które przyzna żądanie później niż po 3 s, gdy lightbox już wyszedł z pełnego ekranu, zostaje na pełnym ekranie.
- Zrobione: fokus. Panel boczny w trakcie wysuwania (200 ms) i lightbox w trakcie wygaszania (50 ms) nie przyjmują fokusu (`sidebar_focused`, `dialog_focused` w `views/lightbox.rs`; `inert` w Solid): fokus wniesiony tam przez Tab idzie na przycisk ponownego otwarcia albo na korzeń okna. Gdy kontrolka dialogu z fokusem zostaje wyłączona albo znika z klatki, fokus bierze dialog, a nie korzeń okna (`LightboxView::focus_lost`, wołane z `on_focus_lost` powłoki w `views/shell.rs`).
- Zrobione: testy bez poprawek kodu. Zwalnianie obrazu (`frees_the_picture_after_navigation_and_once_the_closed_lightbox_has_faded_out`: po nawigacji i po wygaszeniu `Originals` nie trzyma obrazu, `retired_pictures()` to 0 po następnej klatce, a pikseli nie trzyma nikt poza testem), strzałki w polach nazwy grupy i kolejności (`the_arrows_in_the_group_fields_move_the_caret_and_select_no_other_asset`; wiązania były poprawne), skryptowany start lightboxa (`tests/lightbox_window.rs`: `a_scripted_start_opens_the_lightbox_on_its_tile_zoomed_and_dragged`, `a_scripted_start_opens_the_delete_confirmation_with_its_word_typed`). Wiersze w `docs/development/testing.md` („GPUI workspace”), w tym wiersz `media_tagger_gpui::scenario` o skryptowanym lightboxie.
- Odkryte: przycisk z fokusem, który zostaje wyłączony, wypada z klatki GPUI (`ui/button.rs`: wyłączony przycisk nie śledzi uchwytu fokusu), a powłoka przenosiła wtedy fokus na korzeń okna, czyli poza kontekst klawiszy lightboxa. Serce ma fokus od otwarcia (pierwszy przystanek Tab dialogu) i jest wyłączone na czas zapisu ulubionego, więc po każdym przełączeniu ulubionego strzałki, `f`, `0`, `+` i `-` przestawały działać do następnego Tab albo kliknięcia. Tego nie było na liście audytu; naprawione razem z fokusem panelu, test `the_dialog_keeps_its_keys_when_its_focused_control_is_disabled`.
- Odkryte: budżet dekodowania jest zgodny z dokumentami, to nie usterka. GPUI tworzy dekoder przez `ImageReader::into_decoder` (zed `crates/gpui/src/platform.rs:3391-3396`), które woła tylko `set_limits` (image-0.25.10 `src/io/image_reader_type.rs:219-223`); domyślne `set_limits` oraz wersje PNG i JPEG sprawdzają same wymiary (`src/io/decoder.rs:115-120`, `src/codecs/png.rs:264-271`, `src/codecs/jpeg/decoder.rs:168-174`), a domyślne limity nie mają wymiarów (`src/io/limits.rs:50-58`). `max_alloc` 512 MiB trafia tylko do wewnętrznych buforów dekodera PNG (`png.rs:50-51`; png-0.18.1 `src/decoder/mod.rs:390-391`, `stream.rs:783-791`), a bufor wyniku przydziela `decoder_to_vec` bez pytania o limit (`src/io/free_functions.rs:303-316`). Rezerwacja całego obrazu jest w `ImageReader::decode` (`image_reader_type.rs:314-320`), którego GPUI nie woła. `MAX_DIMENSIONS` w zune-jpeg-0.5.15 występuje tylko w tekście błędu. Sprawdzone też w praktyce (jednorazowy test, nie zostaje w repozytorium): syntetyczny PNG RGBA 12000 x 11200 (134,4 Mpx, 537,6 MB po zdekodowaniu) i JPEG 14000 x 13200 (184,8 Mpx) dekodują się przez `media::decode`. Komentarz stałej `MAX_DECODED_PIXELS` i ADR 0046 mówią teraz wprost, że żaden limit dekoderów nie leży poniżej budżetu i że w trakcie dekodowania obraz RGB zajmuje chwilowo więcej niż 4 bajty na piksel. Liczby budżetu bez zmian.
- Odkryte: komunikat nawigacji leży w lewym dolnym rogu kolumny mediów, czyli nad wierszem szyny akcji, nie nad sceną (tak samo w Solid), więc kółko nad nim i wcześniej nie trafiało do sceny; nasłuch dodany dla zgodności z regułą, bez osobnego testu. Scenariusz ma fokus na pierwszym przystanku Tab dialogu (daje go warstwa modalna), bez pierścienia; „nic nie ma fokusu” znaczyło „żadne pole”, zdanie w `docs/subsystems/lightbox.md` i komentarz `ScenarioLightbox` są doprecyzowane.
- Zostało: nic otwartego z listy audytu, raport `reports/E16-review-open.md` nie powstał. Limit czasu odpowiedzi okna (3 s) i sama kolejność odpowiedzi na Wayland nadal nie są sprawdzone na prawdziwym kompozytorze (okno testowe GPUI przełącza pełny ekran synchronicznie); wpis w notatkach E19. Porównanie zrzutów i `test:all` uruchamia orkiestrator: rysowanie nie zostało zmienione.
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client` → pass; `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui` → pass (`lightbox_ported` 48, `lightbox_window` 5); `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings` → pass; każdy nowy test poprawki sprawdzony bez niej (nie przechodzi); `job.py start E16 checks` → pass (dziewięć komend, kod 0).

### 2026-11-23 — orkiestrator (zamknięcie)

- Zrobione: przyczyna blokady bramki usunięta poza epikiem (osobny commit podnosi `cookie` do 0.18.3 w `src-tauri/Cargo.lock`; `bun run audit:rust` przechodzi). Braki z audytu po workflow (osiem pozycji: leady z notatek soczewek, brakujące testy, dokumentacja) zamknięte wpisem „poprawki po audycie orkiestratora” powyżej; raport otwartych ustaleń nie powstał, bo nic nie zostało otwarte.
- Zrobione: porównanie zrzutów pięciu scenariuszy `lightbox-*` powtórzone dwa razy na końcowym kodzie (przed poprawkami audytu i po nich): układ zgodny w `lightbox-image`, `lightbox-image-zoomed` i `lightbox-delete-confirm`, a `lightbox-gif` i `lightbox-video` różnią się tylko sceną mediów (D8). Zrzuty GPUI po poprawkach audytu są bajt w bajt takie same jak przed nimi. Jedyna widoczna różnica to tekst w polach o około 2 px niżej przy identycznych ramkach pól (metryki fontu wspólnego pola tekstowego, D10).
- Zostało: bez sprawdzenia na prawdziwym ekranie zostają pełny ekran na kompozytorze Wayland i menedżerze okien X11 (w tym termin 3 s na odpowiedź okna), okno powiadomienia po usunięciu (`window.prompt`), szybkość kółka na prawdziwej myszy; pozycje są w notatkach E19. Poprawek fokusu z audytu (Tab podczas zanikania, wyłączony przycisk z fokusem) nie sprawdzono dla innych dialogów na `ui::modal::Modal`.
- Testy: `job.py start E16 checks` → pass (dziewięć komend, kod 0); `job.py start E16 compare` (pięć scenariuszy) → pass; `job.py start E16 test-all` → pass (`[E2E] COMPLETE PASS`, kod 0).
