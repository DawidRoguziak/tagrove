# E11: Model zaznaczenia i gesty

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E08, E09.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Zaznaczenie jako specyfikacja rozwiązywana w Rust, z tymi samymi gestami co dziś.

## Zakres

- Model w kliencie: zakresy sesji + ID włączone/wyłączone, epoki zapytania i tożsamości, `keepAlive`, rozwiązywanie zakresów do 10 000 pozycji.
- Gesty w GPUI: klik, Ctrl/Meta-klik, Shift-zakres, zaznaczanie przeciąganiem po 150 ms przytrzymania, Ctrl+A, przełącznik trybu zaznaczania.
- Mutacja selekcji przez port (`apply_selection_mutation`).

## Poza zakresem

- Dok akcji zbiorczych (E14).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/gallery/createSelection.ts`
- `src/features/gallery/selection.ts`
- `src/features/gallery/createGridHandlers.ts`
- `src/features/gallery/selectionGeometry.ts`

Testy do przeniesienia:
- `src/features/gallery/__tests__`

Dokumenty i reguły:
- `docs/subsystems/search-tags-and-media-groups.md`
- `docs/adr/0008-selection-spec-resolved-in-rust.md`
- `docs/adr/0029-range-selection-keeps-its-session-alive.md`

## Kryteria akceptacji

- Testy `createSelection` i `selectionGeometry` przeniesione.
- Zaznaczenie przeżywa odświeżenie zapytania zgodnie z epokami; porażka zapytania je unieważnia.
- Parytet `gallery-selection`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E11 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `gallery-selection`: Trzy zaznaczone kafle (klik, Ctrl-klik, Shift-klik), widoczny dok.

## Ryzyka

- Rozwijanie dużych zakresów do list ID po stronie UI.

## Status

done 2026-11-22

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E07

- Z epiku E07: odczyty zaznaczenia idą przez wejścia modelu zapytań z biletem (`library::Ticket`, wybiera go pytający): `QueryInput::ReadIdRange { ticket, start, end }` (całość albo błąd `QueryError::Cancelled`/`IncompleteRange`/`Backend`, nigdy część zakresu) i `QueryInput::KeepSessionAlive { ticket, session_id }` (`KeepAlive::Ready | Stale | Skipped`). Odpowiedź przychodzi dokładnie raz jako `QueryEvent` w `AppRuntime::take_query_events()`; model zaznaczenia osadź w `AppRuntime` obok zapytania i przekazuj mu te zdarzenia po każdym wejściu zapytania (ADR 0040). Odczyt wyprzedzony przez refresh/reset dostaje odpowiedź od razu (zakres: `Cancelled`, keepalive: `Skipped`).
- Z epiku E07: epoki i sesja jak w Solid: `query.query_epoch()`, `query.session_id()` (brak sesji podczas odświeżania i po nieudanym starcie), `query.page_failure_epoch()`, `query.navigation_snapshot().total`. Timer keepalive (60 s) należy do modelu zaznaczenia jako jego efekt.

### 2026-11-22 — workflow gpui-port-epic

- Zrobione: model klienta `gallery::Selection` (port createSelection.ts: zakresy sesji, ID dołączone/wykluczone, epoki, keepalive 60 s, materializacja do 10 000 pozycji, retry, powiadomienia, `capture()` ze `SelectionSpec`), osadzony w `AppRuntime` (`RuntimeInput::Selection`, ADR 0043). Port `selection_geometry` i maszyny gestów `SelectionGesture` (przytrzymanie 150 ms + 5 px, prostokąt, przewijanie krawędzi, długi dotyk 500 ms, tłumienie kliknięcia). Widok GPUI: przełącznik akcji zbiorczych, klik / Ctrl / Shift, prostokąt, Escape przez nie-modalną warstwę, Ctrl+A, obwódka i znacznik, błąd zaznaczenia z Retry w banerze, 88 px miejsca u dołu. Scenariusz `gallery-selection` w trzech miejscach. Zaktualizowane dokumenty i nowy ADR 0043. Testy: 39 nowych (23 + 3 w kliencie, 13 w GPUI); 22 z 23 przypadków createSelection.test.ts były już pokryte.
- Odkryte: bilety zapytań zaznaczenia zaczynają się od `SELECTION_TICKET_BASE` (1 << 62); inni czytelnicy zdarzeń (lightbox E15) muszą używać biletów poniżej. Dla E14 (dok): `selection().count()/notice()/busy()/ids()/key()/capture()`, `SelectionCapture::spec_mutation`, `SelectionInput::Remove/Clear`; dok mieści się w 88 px pod stopką. Każda kontrolka w viewportcie galerii musi ustawić flagę `control_pressed` albo zatrzymać wciśnięcie. Odstępstwa od Solid: `RectangleCancel` tylko przy trzymanym wciśnięciu; addytywny commit bez zakresów zostawia zaznaczenie bez zmian. Ikony `h-4 w-4` w Tauri renderują się jako 20 px (podobnie `settings_header::BACK_ICON_SIZE`, niezmienione). Ctrl+A i klawisze przewijania nie działają, gdy fokus jest na korzeniu okna (E19); kafle nie są tab-stopami (E19). Pominięte przypadki testów: batching Solid, budżety czasowe, unmount w trakcie przeciągania, Ctrl+A w polach tekstowych (E12), pointer isPrimary i mechanika DOM. Brak testu renderu pikselowego prostokąta i obwódki placeholdera; kursor i hover nie sprawdzone prawdziwym wskaźnikiem. Różnice parity (minor): brak doku (E14), brak pola wyszukiwania i filtrów (E12/E13), zamienione kafle 4 i 5 w pierwszym rzędzie (E07/E09). Nie zaktualizowano linii układu w CLAUDE.md ani `HOT_PATHS` review-scope.
- Zostało: epik zablokowany, bo `test:all` pada w `audit:rust` z powodów niezwiązanych z epikiem: RUSTSEC-2026-0592 (`url` 2.5.8 w src-tauri/Cargo.lock, poprawka w >=2.5.9; ta sama luka w src-gpui/Cargo.lock z `url` 2.5.7 zatrzyma później `audit:gpui`). Potrzebna osobna zmiana zależności albo zatwierdzony `--ignore`. Do naprawienia (defekt z kroku testów, wg raportu review: czysto): `cx.on_release` w `views/gallery.rs` wysyła `RectangleCancel` tylko przy `gesture.held()`; powinno zawsze, inaczej po wizycie w ustawieniach zostaje nieudany zakres z Retry i oczekujący odczyt zakresu; padają 2 testy GPUI (`leaving_the_gallery_drops_a_failed_range_and_its_retry`, `leaving_the_gallery_drops_a_range_read_on_its_way`). Wszystko po `quality` w `test:all` niezweryfikowane. Otwartych uwag review: brak (1 runda, czysto).
- Testy:
  - `cargo fmt --check` i `cargo clippy -D warnings` (src-gpui): pass
  - `cargo test --locked` src-gpui: pass (krok implementacji); po dodaniu testów klient 26 pass, GPUI `gallery_selection` 22 pass / 2 fail (defekt wyżej)
  - `architecture_boundaries`, `gpui:lock-parity`, `test:compare`, `lint`, `test:privacy`, `git diff --check`: pass
  - `compare:ui gallery-selection`: pass, tylko drobne różnice (brak doku, filtrów, kolejność kafli)
  - `bun run test:all`: fail (`audit:rust`, RUSTSEC-2026-0592, niezwiązane z epikiem)

### 2026-11-22 — workflow gpui-port-epic
- Zrobione: wznowienie od kroku bramki (`from: gate`) po wcześniejszej blokadzie; implementacja i testy nie były powtarzane. Bramka `test:all` przeszła w 1 rundzie, bez poprawek.
- Odkryte: brak nowych ustaleń w tym przebiegu; patrz poprzedni wpis tego dnia (bilety zaznaczenia od `SELECTION_TICKET_BASE`, różnice parity do E12/E13/E14, sprawy dla E19). Parity: 0 rund, brak otwartych różnic w danych wyniku.
- Zostało: nic zablokowanego; brak nierozwiązanych błędów, brak otwartych uwag review (review nie było uruchamiane w tym przebiegu), więc bez raportu. Wynik bramki: ok.
- Testy:
  - checks (poprawki: 0, błędy: brak): nie uruchamiano w tym przebiegu
  - `bun run test:all`: pass (bramka, 1 runda)

### 2026-11-22 — orkiestrator
- Zrobione: blokadę bramki usunięto osobną zmianą zależności (commit `3fd34d8`): `url` 2.5.9 i `form_urlencoded` 1.2.3 w obu plikach `Cargo.lock`, bez nowego `--ignore`; potem wznowienie od kroku bramki.
- Odkryte: sprostowanie pierwszego wpisu z tego dnia. Defekt opisany tam jako „do naprawienia" (`cx.on_release` wysyłał `RectangleCancel` tylko przy trzymanym wciśnięciu) został naprawiony w pętli sprawdzeń tego samego przebiegu: handler w `src-gpui/crates/media_tagger_gpui/src/views/gallery.rs` wysyła go zawsze, a testy `leaving_the_gallery_drops_a_failed_range_and_its_retry` i `leaving_the_gallery_drops_a_range_read_on_its_way` przechodzą. Przegląd odbył się po tej poprawce: 1 runda, pełny zakres 37 plików, siedem soczewek, bez ustaleń.
- Zostało: nic. Niesprawdzone prawdziwym wskaźnikiem: kursor i hover; brak testu renderu prostokąta zaznaczenia.
- Testy: `job.py start E11 checks` → pass (8 kroków); `bun run test:all` → pass (bramka po zmianie zależności)
