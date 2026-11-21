# E06: Runtime aplikacji, okno i powłoka

Rozmiar: M. Poziom: standardowy. Zależy od: E02, E05.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Okno z nagłówkiem i przełączaniem widoków, z modelami żyjącymi przez cały czas życia aplikacji.

## Zakres

- `AppRuntime` w kliencie: kontener modeli na czas życia aplikacji, widok `gallery|settings`, motyw, sekwencja startu (odświeżenie biblioteki, skan startowy, lista rootów).
- Okno z dekoracjami po stronie klienta: własne minimalizuj/maksymalizuj/zamknij, 8 krawędzi zmiany rozmiaru, obszar przeciągania (D14).
- Nagłówek: logo, miejsce na wyszukiwarkę, przycisk ustawień, licznik elementów (liczba mnoga), miejsca na suwak kafli i przełącznik trybu zaznaczania.
- Przełączanie motywu i języka na żywo, zapis przez `PrefsStore`.
- Rejestr skrótów (konteksty klawiszy); brak menu kontekstowych, jak w Tauri.
- Wykonawca produkcyjny z E02 podpięty pod executor tła GPUI.

## Poza zakresem

- Zawartość galerii i ustawień.

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/app`
- `src/platform/tauri/window.ts`

Testy do przeniesienia:
- `src/app/__tests__`

Dokumenty i reguły:
- `docs/frontend/architecture-and-ui-conventions.md`
- `docs/architecture/system-overview.md`

## Kryteria akceptacji

- Modele przeżywają przełączenie widoku; widoki tracą tylko stan lokalny.
- Skan startowy rusza raz (single-flight), a jego błąd nie blokuje okna.
- Parytet `shell-empty` i `shell-empty-light`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E06 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `shell-empty`: Pusta biblioteka, widok galerii (pusty stan), motyw ciemny.
- `shell-empty-light`: Jak wyżej, motyw jasny.

## Ryzyka

- Dekoracje po stronie klienta różnią się między X11 i Waylandem: sprawdzić oba backendy.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-20 — implementer E00

- Z epiku E00: `window.rs` otwiera okno z domyślnymi dekoracjami serwera i widokiem-zaślepką `views/hello.rs` (model `media_tagger_client::hello`); dekoracje po stronie klienta (D14) i właściwa powłoka należą do tego epiku, a `hello` można wtedy usunąć razem z testami `tests/hello_window.rs`. Rozmiar 1440×900 i minimum 1000×720 są przeniesione z `src-tauri/tauri.conf.json`; tytuł i `app_id` pochodzą z profilu (`profile.rs`).
- Z epiku E00: `Boot` (AppState + blokada instancji) żyje w `media_tagger_gpui::run` do powrotu z pętli zdarzeń; `startup_scan_roots` czeka na single-flight skanu startowego.

### 2026-11-21 — implementer E04

- Z epiku E04: rejestr skrótów musi trzymać zasady z ADR 0038 (`docs/frontend/architecture-and-ui-conventions.md`, „GPUI port: keys and text fields”): każdy skrót w kontekście klawiszy powłoki albo głębszym, nigdy bez kontekstu; skróty na klawiszach wpisujących znak przez `text_input::outside_text_input(kontekst)`. Ctrl+K wiązany w kontekście powłoki działa także z fokusem w polu (pole go nie używa), jak w Tauri. Wzorzec testu: `tests/text_input.rs` (`shell_bindings`).
- Z epiku E04: wyłączenie pola, które ma fokus (`TextInput::set_disabled`), zdejmuje fokus z okna; bez sfokusowanego elementu w ścieżce nie ma żadnego kontekstu klawiszy i skróty powłoki nie pasują. Powłoka powinna odzyskać fokus w `cx.on_focus_lost(window, …)` (tak robi Workspace w Zed) i mieć własny `FocusHandle` na korzeniu.
- Z epiku E04: `ui::init(cx)` jest już wołane w `window::run_until_closed` przed otwarciem okna; testy na `TestAppContext` wołają je same.
