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
