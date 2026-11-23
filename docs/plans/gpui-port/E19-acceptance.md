# E19: Domknięcie: parytet, wydajność, klawiatura, dokumentacja

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E10, E14, E16, E18.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Potwierdzić, że aplikacja GPUI robi to samo co Tauri (poza D8), i zostawić po sobie dokumentację oraz narzędzia przeglądu.

## Zakres

- Pełny przebieg wszystkich scenariuszy porównania; lista różnic z decyzją „naprawić / świadoma różnica”.
- Przegląd klawiatury i fokusu względem `docs/subsystems/lightbox.md` i konwencji frontendu.
- Wydajność na syntetycznej bazie 1 000 000 pozycji: przewijanie, odświeżenie zapytania, pamięć po długiej sesji.
- Dokumentacja: nowa strona `docs/frontend/gpui-architecture.md`, aktualizacja `system-overview.md`, `testing.md`, `CLAUDE.md`, reguł `.claude/rules/`.
- ADR 0036 → Accepted; wpisy `HOT_PATHS` dla plików siatki i miniatur sprawdzone na istniejących ścieżkach.
- Batche `/audit` (`.claude/skills/audit/references/batches.json`) i jego test pokrycia obejmują źródła w `src-gpui/`.
- Lista braków → epiki odroczone (E20–E22) z opisem stanu.

## Poza zakresem

- Usunięcie Tauri i identyfikator wydania (E22).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `docs/plans/gpui-port/README.md`
- `e2e/gallery-scale`
- `src-tauri/tests/grid_baseline.rs`

Testy do przeniesienia:
- brak

Dokumenty i reguły:
- `docs/frontend/architecture-and-ui-conventions.md`
- `docs/development/testing.md`
- `docs/adr/README.md`

## Kryteria akceptacji

- Każdy scenariusz ma werdykt; brak różnic „blocking”.
- Pomiar wydajności zapisany w notatkach z porównaniem do Tauri na tym samym seedzie.
- `bun run test:all` zielone; dokumentacja zgodna ze źródłem.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E19 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`
- `python3 -m unittest discover -s .claude/skills/review-changes/scripts -p test_collect_scope.py`

## Scenariusze porównania

- brak (epik bez widocznego odpowiednika w Tauri albo bez UI)

## Ryzyka

- brak

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-22 — implementer E14

- Z epiku E14: nieprzeniesione szczegóły doku do oceny przy domknięciu: ogłoszenie przesunięcia kafla dla czytników ekranu (`bulk.orderModal.moved`, w Solid `aria-live`), podpowiedzi `title` (serce, licznik, nazwa pliku na kaflu planszy), mały spinner kafla planszy (użyty spinner kafla siatki), grubość kreski ikony zamknięcia w liczniku (2.4 w Tauri, 1.8 w zasobie ikony). Klik w puste miejsce doku nie zdejmuje fokusu z pola (w przeglądarce fokus wraca na `body`).

### 2026-11-22 — poprawki E14 (audyt orkiestratora)

- Z epiku E14: lista sugestii pól tagów (`views::search::suggestion_list_body`, wspólna dla nagłówka wyszukiwarki i `TagEditor`) nie ma roli ani nazwy dostępnej, a `TagEditorProps` nie ma na nią pola. W Solid to `role="listbox"` z `aria-label` (`listboxAriaLabel`; w doku `bulk.tagModal.suggestions`), po której test „removes and automatically adds tags for a single selected asset” znajduje listę. Port tego testu (`tests/bulk_dock_ported.rs`) znajduje listę po selektorze; nazwa listy jest do oceny przy domknięciu dostępności.

### 2026-11-22 — implementer E16

- Z epiku E16: różnice lightboxa względem Tauri zostawione do domknięcia (lista także w `docs/subsystems/lightbox.md`, „Known limitations”): obraz ponad budżet dekodowania (bok > 16 384 px, > 192 Mpx, plik > 512 MiB) pokazuje błąd mediów zamiast pomniejszonego obrazu, bo pomniejszenie wymaga zależności `image` w adapterze (osobna decyzja, ADR 0046); mocno pomniejszony obraz jest próbkowany bez mipmap; GPUI nie ma `letter-spacing`, więc nagłówek potwierdzenia usunięcia łamie się na dwie linie przy szerokości 340 px; kolejność Tab stawia pływające przyciski (otwórz panel, zamknij) po szynie akcji, bo muszą być rysowane nad sceną; kursor `zoom-in` nie istnieje w GPUI (jest rączka); urządzenie tylko dotykowe nie jest wykrywane (`Session::new(..., touch_device = false)`); jeden krok kółka to delta GPUI przeliczona po 16 px na linię, co może dawać wolniejszy zoom niż WebKit (do pomiaru na prawdziwej myszy); powiadomienie po usunięciu (brak źródła, sprzątanie w toku) to `window.prompt` GPUI z przyciskiem Close.
- Z epiku E16: testy okna muszą po `settle` narysować klatkę (`window.simulate_next_frame`), bo fokus lightboxa i slajd panelu ruszają klatkę później (wzorzec w `tests/lightbox_window.rs`). Zamknięty lightbox blokuje wskaźnik do końca wygaszania (50 ms), więc test klikający siatkę po zamknięciu przesuwa zegar.

### 2026-11-23 — poprawki po audycie orkiestratora (E16)

- Z epiku E16: `lightbox::WindowFullscreen` zapomina żądanie pełnego ekranu, na które okno nie odpowiedziało w 3 s (`WINDOW_ANSWER_TIMEOUT`). Na prawdziwym kompozytorze Wayland i na menedżerze okien X11 trzeba sprawdzić: czy odpowiedź przychodzi w tym czasie, czy `f` i Escape wciśnięte szybko po sobie zostawiają okno w oczekiwanym stanie, oraz czy pełny ekran włączony skrótem menedżera okien po odrzuconym żądaniu zostaje. Test okna tego nie pokaże: okno testowe GPUI przełącza pełny ekran synchronicznie.
- Z epiku E16: zamknięty lightbox i wysuwany panel nie przyjmują fokusu dzięki regułom w `views/lightbox.rs` (`dialog_focused`, `sidebar_focused`). Element `ui::modal::Modal` innych dialogów rysuje zamykaną treść przez czas wygaszania tak samo, a `layers::move_focus` bez otwartej warstwy modalnej chodzi po wszystkich przystankach Tab okna; czy Tab w trakcie wygaszania zatrzymuje się tam na kontrolce zamykanego dialogu, nie było sprawdzane (do oceny przy domknięciu dostępności). To samo dotyczy przycisku z fokusem, który zostaje wyłączony w innym dialogu: fokus spada na korzeń okna, poza kontekst klawiszy tego dialogu.

### 2026-11-23 — poprawki po audycie orkiestratora (E17)

- Z epiku E17: kolejność Tab na stronie ustawień GPUI to nagłówek → kontrolki sekcji → linki nawigacji, bo nawigacja jest rysowana po przewijanym kontenerze sekcji (`views/settings.rs`, `render`); w Tauri linki nawigacji idą przed sekcjami. Aktywacja linku przenosi fokus na sekcję i Tab wchodzi w jej kontrolki (naprawione w E17), ale sama kolejność przystanków jest do oceny przy domknięciu klawiatury.
- Z epiku E17: `UiIcon` z `h-4 w-4` rysuje się w Tauri w 20 px (własne `h-5 w-5` ikony leży w arkuszu później). Nawigacja ustawień została poprawiona; strzałka „Back” nagłówka ustawień (`shell::settings_header::BACK_ICON_SIZE` = 16, epik E06) nadal ma 16 px i razem z przesunięciem tytułu (ok. 6 px) oraz dolną krawędzią nagłówka (y 74 zamiast 72) jest do poprawienia przy parytecie. Krawędź aktywnego linku nawigacji jest w GPUI prostym paskiem 2 px, w Tauri idzie po zaokrągleniu linku.
- Z epiku E17 (sprostowanie pierwszego punktu tego wpisu): kolejność Tab na stronie ustawień została naprawiona w E17 i nie jest już otwarta. Nawigacja jest rysowana przed sekcjami, więc Tab idzie nagłówek → linki nawigacji → kontrolki sekcji, jak w Tauri (test `tab_goes_through_the_header_the_navigation_and_then_the_sections` w `tests/settings_window.rs`). Otwarte z tego wpisu zostają strzałka „Back” z pozostałymi różnicami nagłówka ustawień i krawędź aktywnego linku nawigacji.
