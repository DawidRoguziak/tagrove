# E14: Dok akcji zbiorczych i kolejność grup

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E11, E13.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Pływający dok dla zaznaczenia: tagi oraz grupa i kolejność (ADR 0035).

## Zakres

- Model `createBulk` i `createSelectedSummaries` w kliencie; serwisy scalania tagów i kolejności grup.
- Dok: tablist z ruchomym tabindexem (strzałki, Home/End); zakładka tagów (jeden zasób: zamiana, wiele: tylko dodawanie); zakładka grupy (klucz, generowany UUID, zastosuj/usuń, komunikat o konflikcie).
- `GroupOrderBoard`: przeciąganie do zmiany kolejności, strzałki, góra/dół o wiersz; inline albo w modalu, nigdy oba naraz.

## Poza zakresem

- brak

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/bulk`
- `src/features/lightbox/parseMediaGroupOrder.ts`

Testy do przeniesienia:
- `src/features/bulk/__tests__`

Dokumenty i reguły:
- `docs/subsystems/search-tags-and-media-groups.md`
- `docs/adr/0035-bulk-actions-floating-dock.md`
- `docs/adr/0026-set-based-ranged-group-write-with-index-rebuild.md`

## Kryteria akceptacji

- Testy modeli i serwisów bulk przeniesione.
- Częściowy wynik publikuje tylko potwierdzone ID.
- Parytet `bulk-dock-tags`, `bulk-dock-group`, `group-order-modal`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E14 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `bulk-dock-tags`: Dok, zakładka tagów, wiele zaznaczonych zasobów.
- `bulk-dock-group`: Dok, zakładka grupy i kolejności z planszą inline.
- `group-order-modal`: Plansza kolejności w modalu.

## Ryzyka

- Przeciąganie z klawiaturą: łatwo zgubić fokus po zmianie kolejności.

## Status

done 2026-11-22

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-22 — poprawki (checks, runda 1)
- Zrobione: ponowny odczyt pełnej listy tagów czeka na koniec trwającego odświeżenia zapytania, a dopiero potem na `waitForIdle` (`ReadPhase::WaitingQuery`, `Bulk::wait_to_retry` w `src-gpui/crates/media_tagger_client/src/bulk/model.rs`).
- Odkryte: model biblioteki odpowiada na odrzucony odczyt szczegółów już w chwili zmiany epoki zapytania (E08, `DetailsCache::clear`), a odświeżenie zmienia epokę dwa razy (start i wynik). Ponowienie zaraz po pierwszej zmianie było odrzucane przez drugą i zużywało jedną z trzech prób: trzy wywołania `get_asset_details` tam, gdzie Solid robi dwa.
- Zostało: nic z tej rundy.
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client` → pass

### 2026-11-22 — poprawki (parytet, runda 1)
- Zrobione: kafel planszy kolejności był o 1 px za wysoki (inline 162 zamiast 161 px, w modalu 261 zamiast 260): `BoardLayout::tile_height` doliczał linię podpisu, a `h-11` podpisu już ją zawiera (border-box). Usunięta stała `CAPTION_RULE` (`src-gpui/crates/media_tagger_client/src/bulk/board.rs`, `bulk/layout.rs`). Po poprawce dolna krawędź kafla leży na tym samym pikselu co w Tauri (y 675 inline, y 412 w modalu, zmierzone na nowych zrzutach). Testy stanu startowego scenariuszy grupy sprawdzają teraz kolejność planszy na deterministycznej bibliotece (`tests/bulk_dock_window.rs`).
- Odkryte: obcięcie etykiety („control-3.p…”, „control-pla…”) nie wynika z węższego pola: pole nazwy ma po obu stronach 93,3 px (kafel 174,5 px, ramka 2, padding 24, numer, dwa odstępy po 8, uchwyt 32). WebKitGTK pod Xvfb rysuje JetBrains Mono 12 px z krokiem zaokrąglonym przez hinting do 7 px (13 znaków = 91 px, mieści się), GPUI z krokiem kroju 7,2 px (13 znaków = 93,6 px, o 0,3 px za dużo). To rasteryzacja fontów, poza parytetem wg D10, tak jak szerokości tekstu w E06 i E12; kodu nie zmieniono.
- Odkryte: inne pliki i zamienione wideo na planszy to nie błąd kolejności planszy. Shift-klik zastępuje zaznaczenie zakresem trzech pierwszych kafli, więc po obu stronach plansza idzie w kolejności galerii (Solid: `src/features/gallery/createSelection.ts`, `addRanges` bez `additive`; GPUI: `gallery/selection.rs`, `on_id_range_read`); test na bibliotece z ID = indeks + 1 daje [1, 2, 3]. Różni się sama kolejność galerii: sortowanie to `modified_at` w sekundach malejąco, potem ID malejąco, seed powstaje w około sekundę, a skan nadaje ID w kolejności kończenia pracy przez wątki (`src-tauri/crates/media_tagger_core/src/services/scan_service.rs`). Kolejność zmienia się też między dwoma przebiegami samego Tauri: trzecia karta to raz `control-1.png`, raz `control-4.png`, a w zrzutach wcześniejszych epików PNG leżą za każdym razem inaczej.
- Zostało: deterministyczny seed porównania (różne sekundy `mtime` plików) wymaga zmiany `e2e/fixtures.js` albo `e2e/control.js` po stronie Tauri, czego ten epik nie może ruszać; do decyzji przy E19. Opis w `docs/development/testing.md` („Seed”).
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --lib bulk::` → pass; `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --test bulk_ported --test bulk_core` → pass; `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test bulk_dock_window` → pass; `cargo fmt … --check`, `cargo clippy … -D warnings` → pass; `job.py start E14 compare --scenario bulk-dock-group --scenario group-order-modal` → exit 0

### 2026-11-22 — poprawki (parytet, runda 2)
- Zrobione: nic w kodzie. Zgłoszenie z rundy 2 (krótsza nazwa pliku na karcie planszy inline) to ta sama różnica co w rundzie 1 i wynika z D10.
- Odkryte: pomiar na zrzutach `bulk-dock-group` potwierdza wyjaśnienie z rundy 1. Tusz etykiety w Tauri ma 89 px na 13 glifów (krok 7 px), w GPUI 85 px na 12 glifów (krok 7,2 px, czyli 600/1000 em kroju JetBrains Mono przy 12 px). W polu wyszukiwania (13 px) jest odwrotnie: Tauri 181 px, GPUI 177 px na 23 znaki (krok 8 px wobec 7,8 px), więc WebKitGTK pod Xvfb zaokrągla krok glifu do całych pikseli, a GPUI trzyma metrykę kroju. Pole nazwy ma w obu aplikacjach te same wymiary (`measures::CAPTION_*`, `HANDLE_SIZE` w `src-gpui/crates/media_tagger_client/src/bulk/layout.rs` wobec `px-3 gap-2 w-8` w `src/features/bulk/GroupOrderBoard.tsx`); 13 znaków przekracza je w GPUI o 0,3 px. Poszerzenie pola albo zwężenie tekstu rozjechałoby układ z Tauri tam, gdzie WebKit nie zaokrągla kroku.
- Zostało: nic z tej rundy; różnica zostaje jako rasteryzacja fontów (D10).
- Testy: brak zmian w kodzie, nic nie uruchomiono.

### 2026-11-22 — workflow gpui-port-epic
- Zrobione: model `bulk` w crate klienta (`src-gpui/crates/media_tagger_client/src/bulk.rs` i `bulk/`: łączenie tagów, kolejność grup z `parse_media_group_order`, `SelectedSummaries` w paczkach po 256, model `Bulk` w `AppRuntime`, stan `Dock`, geometria planszy i miary układu) oraz widok `views::bulk_dock` w adapterze GPUI (chip i rozwinięty dok, zakładki z jednym tab stopem, zakładka tagów, zakładka grupy z kluczem, UUID, zastosowaniem, usunięciem i alertem konfliktu, plansza inline i większy widok, nigdy oba naraz). Widok galerii tworzy dok w trybie zaznaczania. Wyniki częściowe publikują kanoniczne wartości tylko dla potwierdzonych ID. Scenariusze `bulk-dock-tags`, `bulk-dock-group`, `group-order-modal` są w trzech miejscach. Dokumentacja: sekcja "GPUI port: bulk dock and group order" w `docs/subsystems/search-tags-and-media-groups.md`, ADR 0044, testing.md, system-overview.md, architecture-and-ui-conventions.md, notatki w E15, E16, E19. Testy: 77 w `tests/bulk_ported.rs` (klient), 13 w `tests/bulk_dock_ported.rs` (GPUI), plus `bulk_core.rs` i `bulk_dock_window.rs`; sportowano 84 przypadki TS. Bez zmian w core, `src-tauri`, `src/`, `Cargo.lock`.
- Odkryte: ADR 0044: `Bulk` nie implementuje `Model`, czyta bibliotekę i zaznaczenie przez `BulkEnv` i zmienia je tylko efektami; runtime wysyła `BulkInput::Sync` po każdym wejściu. Bilety biblioteki od `BULK_TICKET_BASE` (2^61) do `SELECTION_TICKET_BASE` (2^62) należą do bulk; inni czytelnicy (lightbox, E15) muszą zostać poniżej 2^61. `AppRuntime::take_library_events()` zwraca listę; `Runtime::publish` w GPUI ją opróżnia i porzuca, więc E15/E16 muszą usunąć to drenowanie. Stan lokalny doku należy do widoku `BulkDock`; szkice modelu przeżywają stronę ustawień. UUID v4 z losowo zasianego hashera std (`dock::random_bits`), niekryptograficzny. Miniatury planszy przez drugi zbiór `Thumbnails::sync_board`; lightbox potrzebuje własnego. Kroki skryptowe doku nie dają polom fokusu. Zmiany wspólnych prymitywów: `Modal::height`, `Alert` z dziećmi i selektorem, `Button` z `size` i `height`, pole edytora tagów bez wymuszonego monospace. Zamknięty `Modal` w kolumnie flex z gap musi siedzieć w `absolute().size_0()`. Ikony doku 20 px (`UiIcon` h-5 w-5 wygrywa). Konteksty `BulkDockTab` i `GroupOrderHandle` przechwytują strzałki/Home/End. Odroczone: pełne porty testów były w kroku testów (zrobione), ogłoszenie czytnika ekranu przesuniętego kafla (`bulk.orderModal.moved`), tooltipy `title`, mniejszy spinner kafla planszy i grubość kreski ikony zamknięcia (1,8 zamiast 2,4), kliknięcie w puste miejsce doku nie zdejmuje fokusu z pola (wszystko w E19). Pominięte testy: "removes and automatically adds tags for a single selected asset" (asercje DOM/CSS listy sugestii, pokryte w bulk_dock_window.rs i testach E13); połowa "ignores details completed after deletion" (wewnętrzne `library.tags.remove`, usunięcie należy do epiku lightboxa); wariant "rejected" w `it.each` (w Rust błąd to `TagSettled { saved: false }`, pokryty wariantem "failed"); wpisywanie nowego szkicu w oknie przy oczekującym starym zapisie (pole wyłączone jak w `createBulk`, pokryte testem klienta). Parytet, otwarte: karty planszy inline w `bulk-dock-group` skracają nazwy plików wcześniej niż Tauri (font: krok glifu 7,2 px w GPUI wobec 7 px w WebKit, patrz D10, poprzednie notatki). Drobne: kolejność kafli o tej samej sekundzie modyfikacji różni się między uruchomieniami w obu aplikacjach (trzeci zaznaczony element), więc trzecia karta w `bulk-dock-tags`, `bulk-dock-group`, `group-order-modal` bywa inna (własność E07/E09 lub fixture); wiersz nazwy grupy przesunięty o ok. 5 px, odstęp przed "Apply group" 8 px zamiast 12 px.
- Zostało: wada z kroku testów: `re_reads_complete_tags_when_a_query_refresh_discards_a_pending_detail_read` (`tests/bulk_ported.rs`) padał, bo jedno odświeżenie zapytania przy oczekującym odczycie szczegółów dawało 3 wywołania `get_asset_details` zamiast 2 (`Library::clear_details` odpowiada `Ok(None)` przy pierwszym podbiciu epoki w `refresh()`, a `WaitForIdle` modelu bulk jest odpowiadane od razu; każde odświeżenie zużywa dwie z trzech prób `TAG_READ_ATTEMPTS`). Etap sprawdzeń i recenzji zamknięty: kontrola zakończyła się powodzeniem (1 poprawka), recenzja w 2 rundach czysta (1 potwierdzone, 1 naprawione, nic otwartego), bramka zielona; w wyniku nie ma więc otwartego błędu, a raportu recenzji nie utworzono. Kryterium parytetu: porównanie zrealizowane (3 rundy), jedna otwarta notowana różnica notable (skracanie nazw, rasteryzacja fontów). Niezweryfikowane: ekran rzeczywisty.
- Testy: rust fmt i clippy, `architecture_boundaries`, GPUI fmt i clippy `-D warnings`, lock parity, testy klienta i GPUI (osiem sprawdzeń epiku): pass. `bulk_ported` (77) i `bulk_dock_ported` (13): pass po poprawce (gate ok, 1 runda). `bun run test:all` w bramce: pass. `compare:ui` dla trzech scenariuszy: wykonane, różnice jak wyżej.

### 2026-11-22 — poprawki (audyt orkiestratora)
- Zrobione: (1) Wiersz klucza grupy ma tor przycisku „Generate UUID” jak siatka Solid `grid-cols-[minmax(0,1fr)_40px_auto]`: miara `layout::group::KEY_BUTTON_TRACK` (40 px) i pudełko o tej szerokości wokół przycisku 36 px w `views/bulk_dock.rs` (`key_row`). (2) Zakładka wraca do „Tags” tylko w chwili, gdy zaznaczenie staje się puste, a nie przy każdej synchronizacji z pustym zaznaczeniem (`Dock::sync`, pole `selection_empty` w `src-gpui/crates/media_tagger_client/src/bulk/dock.rs`); tak działa efekt na `selectedCount === 0` w `src/features/bulk/BulkDock.tsx`. Przy komunikacie o wyczyszczonym zaznaczeniu (dok otwarty, 0 zaznaczonych) kliknięta zakładka „Group & order” zostaje, także po ponownym zaznaczeniu. (3) Przeniesiony pominięty przypadek „removes and automatically adds tags for a single selected asset” (`tests/bulk_dock_ported.rs`): usunięcie tagu kontrolką chipa, dodanie Enterem i z listy sugestii oraz odpowiednik asercji położenia listy. (4) Nowe testy okna: tor wiersza klucza, Escape w polu tagów i w polu klucza grupy, publikacja runtime bez zmian nie rusza pola klucza (`tests/bulk_dock_window.rs`); zawijanie długiego komunikatu w wąskim alercie i rozmiary przycisku (`tests/controls.rs`). (5) Dokumentacja: sekcja „GPUI port: bulk dock and group order” (zakładka, wiersz klucza, Escape) i opisy plików testów w `docs/development/testing.md`. Notatki „Z epiku E14” dopisane w E15 (połowa testu z usunięciem zasobu) i E19 (nazwa dostępna listy sugestii).
- Odkryte: pomiar `bulk-dock-group` w wierszu y = 429 po poprawce: Tauri ramka pola do x = 931, przycisk 940–975, „Apply group” 988–1086 (99 px); GPUI ramka pola do x = 932, przycisk 941–976, „Apply group” 989–1086 (98 px). Przed poprawką GPUI: 936, 945–980, 989–1086. Odstępy są teraz te same (8 px za polem, 12 px przed „Apply group”); pozostały 1 px to szerokość napisu „Apply group” (99 wobec 98 px), czyli rasteryzacja fontów (D10).
- Odkryte: Escape działa w GPUI jak w Solid, bez zmian w kodzie. Solid: `SearchTagator.tsx:142` zatrzymuje Escape tylko przy otwartej liście z pozycjami, inaczej klawisz trafia do `UiLayerProvider` (`window`, faza bąbelkowania), którego górną warstwą w trybie zaznaczania jest galeria (`createGridHandlers.ts:105`, `onEscape: clearSelection`). GPUI: `text_input::Cancel` → `views/tag_editor.rs:495` (`Combobox::key`, `search/combobox.rs:411`) → wiązanie `Dismiss` korzenia (`ui/layers.rs:51`) → warstwa zaznaczenia (`views/gallery/selection.rs:308`). Pole klucza grupy nie ma własnej obsługi, więc Escape od razu czyści zaznaczenie; tryb zaznaczania zostaje.
- Odkryte: `Alert` z ciałem `flex_col().items_start()` zawija komunikat na wewnętrznej szerokości alertu we wszystkich miejscach użycia (pudełko blokowe jak w nagłówku wyszukiwarki, kolumna z odstępem jak w kit, doku i większym widoku, tytuł z przyciskiem); wysokość alertu równa się wysokości tego samego tekstu w pudełku o szerokości treści. `Button`: `size` i `height` naraz ustawia tylko serce doku (`views/bulk_dock.rs`), pozostali wołający ustawiają jedno z nich, więc zamiana kolejności w `ui/button.rs:220` niczego im nie zmienia. `TextInput::set_disabled` wraca od razu przy tej samej wartości (`ui/text_input.rs:350`), a `set_text` w `BulkDock::apply` jest wołane tylko przy innym tekście.
- Odkryte: testy okna doku nie zależą od liczby klatek w `settle`/`park`: oba pliki (29 testów) przechodzą z 1, 2, 3, 4, 6 i 9 klatkami na krok (sprawdzone tymczasową zmianą pętli, przywróconą). Czekać „do braku wywołań zwrotnych klatki” się nie da: animacja modala i spinnery proszą o kolejną klatkę, dopóki zegar testu stoi. Kodu testów nie zmieniono.
- Odkryte: scenariusze `bulk-dock-group` i `group-order-modal` nie potrzebują czekania na odblokowane pole klucza. Pole blokują tylko `selectedCount === 0`, `selectionBusy` i `groupApplying` (`BulkDock.tsx:367`); krok przed `fill` czeka na `data-selected-count="3"`, a `createSelection.ts` publikuje liczbę i zdejmuje `busy` w tym samym przebiegu (`finish`, potem `finally` w `addRanges`); kliknięcia licznika wyników i zakładki nie zaczynają gestu prostokąta. Pole tagów w `bulk-dock-tags` czeka, bo blokuje je asynchroniczny odczyt szczegółów. Plików scenariuszy nie zmieniono.
- Zostało: z testu „removes and automatically adds tags …” nie da się wyrazić dwóch asercji: nazwy listy („Bulk tagging suggestions”), bo lista sugestii GPUI nie ma roli ani nazwy dostępnej (notatka w E19), oraz klas CSS `fixed` i `bottom-[calc(100%+8px)]`, które zastępuje sprawdzenie granic listy i trafienia w opcję poza pudełkiem doku. Połowa testu z usunięciem zasobu czeka na epik lightboxa (notatka w E15). Niezweryfikowane: ekran rzeczywisty; scenariusze `bulk-dock-tags` i `group-order-modal` nie były ponownie porównywane (zmiana rysowania dotyczy tylko wiersza klucza na zakładce grupy, widocznego także pod modalem w `group-order-modal`).
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --test bulk_ported` → pass (78); `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test bulk_dock_window --test bulk_dock_ported` → pass (15 + 14); `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test controls` → pass (14); `job.py start E14 compare --scenario bulk-dock-group` → exit 0; `job.py start E14 checks` → pass (8 kroków); `bun run test:compare` → pass (50); `git diff --check` → pass.

### 2026-11-22 — orkiestrator
- Zrobione: audyt wyniku workflow na podstawie dziennika agentów: zakres 47 z 47 plików w obu rundach przeglądu, każda soczewka zwróciła wynik w obu rundach, żadne znalezisko nie odpadło przez pewność poniżej 60. Braki z audytu naprawił agent (wpis wyżej); po jego zmianach `job.py start E14 checks` i `job.py start E14 test-all` przeszły.
- Odkryte: sprostowanie wpisu workflow. Wada z kroku testów (`re_reads_complete_tags_when_a_query_refresh_discards_a_pending_detail_read`) opisana tam pod „Zostało” jest naprawiona w rundzie checks 1 (`ReadPhase::WaitingQuery`) i nic z niej nie zostało. Drobna różnica wiersza nazwy grupy (odstęp 8 zamiast 12 px przed „Apply group”) też jest już naprawiona (`group::KEY_BUTTON_TRACK`); na nowym zrzucie `bulk-dock-group` przycisk leży 1 px od pozycji z Tauri.
- Zostało: skracanie nazw plików na kaflach planszy (rasteryzacja fontów, D10); deterministyczny seed porównania (decyzja przy E19); pozycje odroczone do E19 i połowa testu usuwania przekazana do E15. Po poprawce wiersza klucza ponownie porównano tylko `bulk-dock-group`; `bulk-dock-tags` i `group-order-modal` mają zrzuty sprzed poprawki (wiersz klucza jest tam niewidoczny albo przykryty dialogiem).
- Testy: `job.py start E14 checks` → pass (8 kroków); `job.py start E14 test-all` → pass (`[E2E] COMPLETE PASS`, exit 0)
