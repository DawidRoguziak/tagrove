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

done 2026-11-24

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

### 2026-11-24 — poprawki po audycie orkiestratora (E18)

- Z epiku E18: w resolverze duplikatów zostaje 1 px różnicy na ramce pola nazwy, wierszu przycisków i dolnych krawędziach kart (GPUI niżej). Linia nazwy grupy (`text-sm`) ma 18,571 px, więc karta pliku zaczyna się na 233,571; GPUI zaokrągla każdą krawędź do najbliższego piksela, a WebKit rysuje górę karty na 234 i pudełka w środku piksel wyżej. Teksty leżą w tych samych wierszach pikseli. Do oceny przy parytecie, razem z kartą „Danger zone” (E17), która w GPUI jest o 1 px niższa (linia 19,5 px nagłówka `text-base`).
- Z epiku E18: `ui::theme::focus_ring` liczy pierścień od pudełka wypełnienia rodzica, więc na elemencie z obwódką leży 1 px bliżej niż obrys CSS. Pole nazwy resolvera kompensuje to lokalnie (`views/settings/duplicates.rs`, `rename_field`); pozostałe użycia na elementach z obwódką są do sprawdzenia.
- Z epiku E18: wyłączone pole nazwy resolvera ma tekst o kryciu ok. 0,30, a w Tauri ok. 0,22 (`input:disabled` razy 40 % koloru tekstu z daisyUI `.input:disabled`); ramka zgadza się (0,55). Obrys fokusu tego pola i pisanie w nim nie były sprawdzane na prawdziwym ekranie.
- Z epiku E18: przywracanie czekające na potwierdzenie jest porzucane przy opuszczeniu strony ustawień (`Settings::page_left`). Kontroler Solid trzymałby je dalej; stan jest w obu aplikacjach nieosiągalny dla użytkownika, bo dialog zakrywa stronę i bierze Escape. Jeśli domknięcie doda inną drogę wyjścia ze strony (skrót, przycisk poza stroną), ta reguła ma zostać.

### 2026-11-24 — implementer E19

- Zrobione (parytet): pełny przebieg 23 scenariuszy porównania (`job.py start E19 compare`, dwa razy: przed poprawkami i po nich, oba z kodem 0 i kompletem zrzutów). Wszystkie pary obejrzane; różnic blokujących brak. Werdykty po drugim przebiegu:

  | Scenariusz | Werdykt | Uwagi |
  | --- | --- | --- |
  | `shell-empty`, `shell-empty-light` | zgodny po poprawce | pierścień kafla ikony pustego stanu rysowany na zewnątrz kafla (66 px jak `ring-1`, było 64 px) |
  | `gallery-default`, `gallery-tiles-small` | zgodny po poprawce | ikona końca wyników 20 px (było 24 px: `size-6` przegrywa z `h-5 w-5` w `UiIcon`) |
  | `gallery-tiles-large` | zgodny, świadoma różnica | własny pasek pozycji GPUI zamiast nakładkowego paska WebKit (ADR 0042) |
  | `gallery-selection`, `bulk-dock-tags` | zgodny po poprawce | krzyżyk licznika doku z kreską 2,4 (było 1,8) |
  | `search-suggestions` | zgodny, świadome różnice | kursor w polu widoczny tylko w GPUI (WebKit nie rysuje go w oknie bez fokusu wejścia), lista 2 px szersza i wiersze 35 zamiast 35,5 px (szerokości tekstu, zaokrąglenia) |
  | `search-applied` | zgodny | |
  | `tag-list-modal` | zgodny po poprawce | pole filtra 40,57 px (było 42 px: `text-sm` to 13 px, nie 14 px); na zrzucie Tauri natywna podpowiedź `title`, której GPUI nie ma |
  | `bulk-dock-group`, `group-order-modal` | zgodny, świadoma różnica | nazwa pliku obcięta znak wcześniej (krok glifu 7,2 wobec 7 px, D10) |
  | `lightbox-image`, `lightbox-delete-confirm` | zgodny po poprawce | tekst pól klucza grupy, kolejności i słowa potwierdzenia leżał 3 px poniżej środka pola (patrz „Odkryte”) |
  | `lightbox-image-zoomed` | zgodny | krawędzie obrazu w tych samych pikselach; różni się tylko rozmycie próbkowania |
  | `lightbox-gif` | świadoma różnica (D8) | chip i pierwsza klatka; krawędzie małego powiększonego obrazu wygaszane na ok. 5 px |
  | `lightbox-video` | świadoma różnica (D8) | zaślepka zamiast odtwarzacza |
  | `settings-scan`, `settings-appearance-language`, `settings-import-export`, `settings-duplicates` | zgodny po poprawce | nagłówek ustawień: strzałka „Back” 20 px, odstęp 8 px, linia opisu 16 px; nagłówek ma 72 px (było 74) i blok tytułu nie jest przesunięty o 6 px |
  | `settings-danger-about` | zgodny po poprawce | jak wyżej; krawędź aktywnego linku nawigacji idzie po zaokrągleniu (cień wewnętrzny zamiast paska); karta „Danger zone” 1 px niższa (subpiksel) |
  | `duplicates-resolver` | zgodny, świadoma różnica | 1 px na ramce pola i dolnych krawędziach kart (subpiksel) |

  Lista różnic świadomych z uzasadnieniem jest w `docs/frontend/gpui-architecture.md` („Parity with the Tauri interface”).
- Zrobione (klawiatura i fokus): przegląd względem `docs/subsystems/lightbox.md` i `docs/frontend/architecture-and-ui-conventions.md`; tabela reguł z pokryciem testami w `docs/frontend/gpui-architecture.md` („Keyboard and focus”). Dwie poprawki: Tab i Shift+Tab omijają przystanki dialogu w trakcie wygaszania (`ui::layers` pamięta kontenery zwolnionych warstw modalnych, dopóki są rysowane; otwarta pozycja z notatki E16), a listy sugestii mają rolę `ListBox` z nazwą (`search.tagSuggestions`, `lightbox.taggingSuggestions`, `bulk.tagModal.suggestions`; nowe pole `TagEditorProps::list_label`) i opcje `ListBoxOption` (otwarta pozycja z notatki E14). Przycisk z fokusem wyłączony w dialogu na `ui::modal::Modal`: fokus idzie na korzeń warstw, skąd Escape zamyka górny dialog, a Tab wraca do niego; żaden z tych dialogów nie ma własnych klawiszy na kontenerze, więc poprawka jak w lightboxie nie jest potrzebna.
- Zrobione (wydajność): pomiar `tests/million.rs` (adapter, ignorowany, bez progów) na prawdziwym rdzeniu z biblioteką 1 000 000 syntetycznych pozycji zasianą tym samym SQL co `src-tauri/tests/grid_baseline.rs` (`TempCore::seed_synthetic_library`). Jeden przebieg w trybie release na jednym hoście (16 wątków, profil tymczasowy na dysku), obok `query_rebuild_and_page_baseline` z `grid_baseline.rs` na tym samym hoście w tej samej godzinie (wywołania rdzenia, które wykonują komendy Tauri, bez IPC i webview):

  | Co | GPUI | Tauri: rdzeń (`grid_baseline`, p50) |
  | --- | --- | --- |
  | klatka przy przewijaniu 48 px na klatkę, 72 kafle | średnia 2,1 ms, p95 2,3 ms, maks. 3,4 ms | nie dotyczy |
  | to samo, 126 kafli | średnia 3,3 ms, p95 3,7 ms, maks. 5,4 ms | nie dotyczy |
  | klatka przy 900 px na klatkę (pauza zapotrzebowania) | średnia 1,1 ms, p95 3,4 ms | nie dotyczy |
  | strona 128 wierszy | mediana 0,32 ms | 0,13 ms |
  | otwarcie okna | 18 ms w wątku UI; pierwsze zapytanie 75 ms, sekcje miesięcy 31 ms | sekcje 27 ms |
  | odświeżenie bez zmian | 10 ms w wątku UI, start zapytania 0,15 ms | 0,09 ms |
  | odświeżenie po zapisie zmieniającym kolejność | 11 ms w wątku UI, start 29 ms, sekcje 28 ms | 32 ms, sekcje 27 ms |
  | nowe zapytanie: ulubione (50 000) | 48 ms | 48 ms |
  | tag `common` (200 000) | 96 ms | 99 ms |
  | tag `rare` (5 000) | 3,9 ms | 6,2 ms |
  | bez `common` (800 000) | 405 ms | 403 ms |
  | wideo (50 000) | 35 ms | 35 ms |
  | skok na koniec i na początek | 1,7 i 2,4 ms w wątku UI (mediana), strony 0,3 ms | nie dotyczy |

  Klatka to zdarzenie, praca modelu, wywołania zwrotne poprzedniej klatki i `Window::draw` na platformie testowej GPUI (układ i malowanie sceny bez GPU, bez rasteryzacji). Z miniaturami odpowiadanymi od razu p95 klatki to 8 ms (72 kafle), 14 ms (126 kafli) i 60 ms przy 900 px na klatkę, bo platforma testowa dekoduje JPEG-i 390 px w wątku testu; aplikacja dekoduje poza wątkiem UI. Pamięć procesu testu (zbiór rezydentny / bajty trzymane przez kod Rust, licznik alokatora): 216 / 2 MiB po zasianiu (w tym samym procesie), 420 / 21 MiB po pierwszym zapytaniu, 568 / 280 MiB przy pełnej pamięci podręcznej obrazów (587 miniatur, 256 MiB, jej budżet), 755 / 290 MiB po wszystkich filtrach, a po każdej z 24 tur długiej sesji (tura: 4 600 klatek przewijania, oba skoki, każdy filtr raz; razem ok. 110 000 klatek) 760–797 MiB i stale 289 MiB. Pamięć podręczna stron trzyma swój limit (24 strony), żaden timer modeli nie zostaje. Długa sesja nie powiększa procesu.
- Zrobione (wydajność, frontend Tauri w tej samej skali): `GALLERY_MILLION=1 sh e2e/gallery-scale/run.sh` (WebKitGTK 2.54.2 na prywatnym Xvfb, milion syntetycznych wyników, transport odpowiadający po 12 ms) → `ok`: skok End na osi czasu 19–22 ms (p50 20 ms, limit 300 ms), Home 5–18 ms, zaznacz wszystko 1–2 ms (limit 50 ms). To inny pomiar niż klatka GPUI (zawiera 12 ms transportu i prawdziwą rasteryzację programową), więc nie stoi w jednej tabeli z powyższą.
- Zrobione (dokumentacja i narzędzia): nowa strona `docs/frontend/gpui-architecture.md`; aktualizacje `docs/architecture/system-overview.md`, `docs/development/testing.md`, `docs/frontend/architecture-and-ui-conventions.md`, `docs/subsystems/search-tags-and-media-groups.md`, `docs/subsystems/lightbox.md`, `docs/development/audit/README.md`. ADR 0036 ma status Accepted od 2026-11-20 (plik i indeks `docs/adr/README.md`), niczego nie trzeba było zmieniać. `HOT_PATHS` w `collect_scope.py`: wpisy siatki i miniatur trafiają w istniejące pliki (test `test_every_hot_path_matches_a_file`), dopisane model zapytań z pamięcią stron i wyszukiwarka z listą tagów. Batche `/audit`: 13 nowych (B16a–B16g klient, B17a–B17d adapter z soczewką `gpui`, B17e harness porównawczy, B17f testy GPUI jako opcjonalny), a test pokrycia wymaga, by każde źródło Rust obu crate'ów GPUI było w jakimś batchu. Notatki o stanie i brakach dopisane w E20, E21 i E22.
- Odkryte: przypięte GPUI maluje element `deferred` (każdy dialog, lightbox, listy sugestii) bez stylów tekstu przodków spoza niego: stos stylów jest odtwarzany dla prepaint, a dla paint nie (`paint_deferred_draws` w `crates/gpui/src/window.rs` przypiętego commita). Pole tekstowe czytało `window.line_height()` w `paint` i dostawało domyślne 26 px zamiast 19,5 px, więc tekst w polach dialogów leżał 3 px za nisko. To była prawdziwa przyczyna różnicy zapisanej w E16 i E18 jako „metryki fontu”. Poprawka: wysokość linii czytana w prepaint (`ui/text_input.rs`, `PrepaintState::line_height`), test `the_fields_of_the_dialog_draw_their_text_in_the_line_height_around_them` (bez poprawki: 26 px). Reguła dla własnych elementów jest w `docs/frontend/gpui-architecture.md` („Traps of the pinned GPUI”).
- Odkryte: w generowanym arkuszu Tauri `h-5 w-5` z `UiIcon` wygrywa także z `size-6`, nie tylko z `h-4 w-4`; rozmiar ikony trzeba brać ze zrzutu, nie z klasy. `[stroke-width:2.4]` wygrywa z `[stroke-width:1.8]`; adapter ma teraz wariant ikony „heavy” (`icons/heavy/<nazwa>.svg`, wyprowadzany przy ładowaniu jak wariant wypełniony).
- Odkryte: test bez pętli klatek musi przed `Window::draw` dostarczyć klatkę (`window.simulate_next_frame`), inaczej wywołania zwrotne `on_next_frame` animowanych elementów zbierają się bez końca. W pierwszych przebiegach pomiaru wyglądało to jak wyciek ok. 5 MiB na turę; licznik alokatora ze śladem stosu dużych alokacji (`MEDIATAGGER_GPUI_PERF_TRACE_MIB`) pokazał `Window::request_animation_frame`. Starszy pomiar `frame_time_while_scrolling_a_large_library` (100 000 wyników) rysuje bez dostarczania klatek; jego czasy klatek to nie zmienia, ale nie nadaje się do oceny pamięci.
- Odkryte: rdzeń nie przebudowuje listy ID zapytania bez filtra po zapisie ulubionego (zmiana nie dotyka kolejności ani członkostwa), więc „odświeżenie po zapisie” w pomiarze używa zapisu grupy mediów, który zmienia kolejność.
- Zostało: `CLAUDE.md` i `.claude/rules/` nie zostały zmienione, choć zakres epiku je wymienia: zmiana instrukcji agentów wymaga zgody użytkownika, a nie polecenia workflow. Do dopisania przez użytkownika albo za jego zgodą: wiersz indeksu dokumentacji `docs/frontend/gpui-architecture.md` w `CLAUDE.md`, komenda pomiaru (`--test million`) w sekcji komend, wskazanie nowej strony w `.claude/rules/gpui-port.md` i `.claude/rules/frontend.md`. Z tego samego powodu `.claude/skills/audit/SKILL.md` nadal podaje „about 65 finder agents” (jest 95).
- Zostało: braki bez epiku w tym planie są spisane w notatkach E22 (paski przewijania poza galerią, podpowiedzi `title`, regiony `aria-live`, fonty, sprawdzenia na prawdziwym ekranie, E2E dla okna GPUI) oraz E20 i E21. Nie zmierzono: klatki z rasteryzacją na prawdziwym GPU, pamięci prawdziwej binarki GPUI na ekranie, całej aplikacji Tauri na tej samej bazie SQLite (`app:control` nie wykonuje skryptu w oknie, więc po stronie Tauri są wywołania rdzenia i osobny fixture frontendu).
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client` → pass; `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui` → pass; `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings` → pass; `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check` → pass; `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries` → pass (33); `bun run gpui:lock-parity` → pass; `python3 -m unittest discover -s .claude/skills/review-changes/scripts -p test_collect_scope.py` → pass (14); `python3 -m unittest discover -s .claude/skills/audit/scripts` → pass; `MEDIATAGGER_GPUI_REQUIRE_RENDERER=1 cargo test … --test settings_render` → pass (zrzuty obejrzane); `TMPDIR=/var/tmp cargo test --manifest-path src-gpui/Cargo.toml --locked --release -p media_tagger_gpui --test million -- --ignored --nocapture` → pass (pomiar); `TMPDIR=/var/tmp cargo test --manifest-path src-tauri/Cargo.toml --locked --release --test grid_baseline query_rebuild_and_page_baseline -- --ignored --nocapture --test-threads=1` → pass (pomiar); `GALLERY_MILLION=1 sh e2e/gallery-scale/run.sh` → ok; `job.py start E19 compare` (23 scenariusze) → kod 0. Nie uruchamiano: `bun run test:all` (bramka workflow).

### 2026-11-24 — workflow gpui-port-epic
- Zrobione: zakres E19 zbudowany, oba crate'y GPUI się kompilują. Parytet: pełny przebieg 23 scenariuszy porównania (przed i po poprawkach), bez różnic blokujących; poprawiono nagłówek ustawień (72 px), znacznik końca wyników (20 px), obwódkę ikony pustego stanu (66 px), filtr listy tagów (40,57 px), tekst pól w dialogach (3 px za nisko), grubszy krzyżyk licznika docka i przycisków usuwania tagów, krawędź aktywnego linku nawigacji ustawień. Klawiatura i fokus: Tab i Shift+Tab omijają dialog w trakcie wygaszania, listy sugestii mają rolę `ListBox` z nazwą (`TagEditorProps::list_label`). Wydajność: ignorowany pomiar `tests/million.rs` na 1 000 000 wyników (klatka 2,1 ms przy 72 kaflach, 3,3 ms przy 126; pamięć stała w 24 turach; zapytania na poziomie `grid_baseline` Tauri). Dokumentacja: nowa strona `docs/frontend/gpui-architecture.md`, aktualizacje sześciu stron, 13 nowych batchy `/audit` z soczewką `gpui`. Testy: 10 nowych testów GPUI (2 przeniesione z `e2e/gallery-scale/million.tsx`: End/Home na osi czasu i Ctrl+A przy milionie wyników; 8 pokrywa poprawki E19). Przegląd: 2 rundy, czysto, 3 potwierdzone i naprawione, 1 odrzucone.
- Odkryte: przypięte GPUI maluje `deferred` bez stylów tekstu przodków, więc własne elementy muszą czytać `window.text_style()` i `window.line_height()` w request_layout lub prepaint, nigdy w paint (`ui/text_input.rs`, `PrepaintState::line_height`); to była prawdziwa przyczyna „metryk fontu” z E16 i E18. `ui::layers` pamięta kontenery zwolnionych warstw modalnych, dopóki są rysowane. Ikony mają trzeci wariant `heavy` (stroke 2,4); rozmiar ikony Tauri brać ze zrzutu, nie z klasy. `TempCore::seed_synthetic_library` kopiuje SQL z `grid_baseline.rs`; oba miejsca trzeba utrzymywać ręcznie w zgodzie. Pomiary na platformie testowej GPUI muszą dostarczać klatkę (`Window::simulate_next_frame`), inaczej wygląda to jak wyciek; starszy test `frame_time_while_scrolling_a_large_library` tego nie robi. Audyt: pełny przebieg to teraz 95 finderów. Pominięte przypadki testowe: limity czasowe WebKit z million.tsx/performance.tsx (to pomiary, nie zachowanie), mechanika height-probe i XTest, rola `ListBox` (platforma testowa GPUI nie buduje drzewa dostępności), stroke 2,4 (źródło zasobów to `()`), rozmiar ikon 20 px i krawędź linku nawigacji (tylko piksele, scenariusz `settings-danger-about`), werdykty scenariuszy i test:all (proces), pamięć w długiej sesji (ignorowany pomiar). Drobne różnice niepoprawione: `focus_ring` 1 px bliżej na elementach z ramką, przezroczystość tekstu wyłączonego pola zmiany nazwy (ok. 0,30 wobec 0,22), kolejność tabulatora pływających przycisków lightboxa, kliknięcie w pustą przestrzeń docka nie zdejmuje fokusu z pola. Luki przekazane do E22 (notatki E22): paski przewijania poza galerią, podpowiedzi `title`, regiony `aria-live`, fonty i odstępy liter, sprawdzenia na prawdziwym Wayland/X11, E2E okna GPUI, zasiew wielomiesięczny.
- Zostało: `CLAUDE.md` i `.claude/rules/` nie zostały zmienione (wymaga zgody użytkownika): wiersz indeksu dla `docs/frontend/gpui-architecture.md`, komenda `--test million`, wskazanie nowej strony w `.claude/rules/gpui-port.md` i `.claude/rules/frontend.md`; `.claude/skills/audit/SKILL.md` nadal podaje „about 65 finder agents” (jest 95). Nie zmierzono: klatki z rasteryzacją na prawdziwym GPU, pamięci binarki GPUI na ekranie, całej aplikacji Tauri na tej samej bazie, select-all przy milionie w oknie GPUI. Brak otwartych ustaleń przeglądu, brak blokady, brak nierozwiązanych błędów; bramka zakończona powodzeniem w 1 rundzie.
- Testy: `bun run test:all` (bramka workflow) → pass; sprawdzenia (checks) → pass, 0 poprawek; testy `media_tagger_client` i `media_tagger_gpui` → pass; clippy `-D warnings` → pass; `cargo fmt --check` → pass; `architecture_boundaries` → pass (33); `gpui:lock-parity` → pass; testy `collect_scope` i skryptów audytu → pass; `settings_render` z rendererem → pass; `compare:ui` 23 scenariusze → pass (kod 0); `GALLERY_MILLION=1 e2e/gallery-scale/run.sh` → pass.

### 2026-11-24 — poprawki po audycie orkiestratora (E19)

- Zrobione (F3, pierścień fokusu): ustalenie prawdziwe dla każdego elementu z obwódką. `ui::theme::focus_ring` był układany od pudełka wypełnienia rodzica (dziecko pozycjonowane absolutnie), a obrys CSS liczy się od pudełka obwódki, więc pierścień leżał o szerokość obwódki za blisko: przycisk miał zewnętrzną krawędź pierścienia 3 px od krawędzi zamiast 4 px (test bez poprawki: `[-3, -3, 73, 35]` wobec `[-4, -4, 74, 36]`). Poprawka centralna: `focus_ring(ring, radius, border: RingBorder)` w `ui/theme.rs`, każde wywołanie podaje obwódki elementu (`RingBorder::NONE`, `all`, `top`, `bottom`); lokalna kompensacja pola nazwy resolvera usunięta. Elementy z obwódką: `ui::Button` (1 px), `ui::Switch` (1 px), licznik doku (1 px), zakładki doku (2 px tylko u dołu), karta motywu (1 px), przełącznik „Information” w panelu lightboxa (1 px tylko u góry), przycisk Retry banera galerii (1 px), pole nazwy resolvera (1 px). Bez obwódki i bez zmiany geometrii: suwak, segmenty, linki nawigacji ustawień, sekcje ustawień, teksty licencji, uchwyt planszy, paski pozycji galerii, przyciski okna, zaznaczony placeholder siatki.
- Zrobione (F3, przy okazji sprawdzenia z CSS): karta motywu używała obrysu bazowego (odstęp 3 px) i przez błąd o 1 px wypadała w tych samych pikselach co `.choice-input:focus-visible + span` (odstęp 2 px od obwódki 1 px); teraz ma odstęp 2 px i obwódkę jawnie (`layout::theme_choice::FOCUS_OUTLINE_OFFSET`, `CARD_BORDER`), piksele bez zmian. Zakładki doku i przełącznik „Information” nie mają w Solid promienia narożników (`.bulk-dock-tab`, `button` bez klasy `rounded`), więc ich obrys ma narożniki proste: `square_focus_ring` zamiast promienia pola.
- Zrobione (F4, wyłączone pole): ustalenie prawdziwe tylko dla pola nazwy resolvera. Wszystkie inne pola Solid to zwykłe `input` z regułą `input:disabled { opacity: 0.55 }` i port rysuje je z kryciem 0,55 (`FieldChrome::Boxed`, `Outlined`, `Bare` w edytorze tagów, którego ramka nie jest przyciemniana). Pole resolvera ma klasę daisyUI `input`: całość 0,55, a kolor tekstu z `.input:disabled` to 40 % `base-content` (0,22). Port przyciemniał dwa razy, ramkę i pole w środku (0,55 × 0,55 = 0,30). Nowy wariant `FieldChrome::Framed` (`text/visuals.rs`): miary jak `Bare`, wyłączone bez własnego krycia, tekst 40 %, placeholder 20 % `base-content`. Ramka i wypełnienie zostają z klas użytkowych (`border-base-content/20 bg-base-100/70`): reguły daisyUI leżą w podwarstwie `utilities` i przegrywają z nimi (sprawdzone w zbudowanym arkuszu).
- Zrobione (F5a, Tab w lightboxie): przypięte GPUI ma grupy Tab (`tab_group()`, `crates/gpui/src/tab_stop.rs`): przystanki grupy idą po przystankach obok niej, niezależnie od kolejności rysowania. Scena, szyna akcji i panel boczny są grupami, dialog też, więc pływające przyciski (otwórz panel, zamknij) idą pierwsze, jak w DOM `LightboxModal.tsx`; piksele bez zmian. Uwaga do pułapek: `tab_index(n)` na elemencie z `id` robi z niego przystanek Tab i cel kliknięcia, więc użyte jest samo `tab_group()`.
- Zrobione (F5b, dok): `BulkDock.tsx` nie blokuje domyślnej akcji żadnego naciśnięcia, więc port: naciśnięcie, którego nie wzięła żadna kontrolka doku, oddaje fokus korzeniowi okna (odpowiednik `body`) i dalej nie idzie (`views/bulk_dock.rs`).
- Zrobione (F6): test `the_ring_of_the_empty_state_icon_tile_lies_outside_the_tile` sprawdza teraz to, co obiecuje komentarz (tytuł 16 px pod kaflem, 15 px pod pierścieniem; nowy selektor `EMPTY_STATE_TITLE_SELECTOR`). Promień pierścienia potwierdzony: `ring-1` to cień z rozrostem 1 px wokół `rounded-[var(--radius-control)]` (6 px), więc 7 px, tyle rysuje port; klasy kafla pilnuje nowy test klienta w `shell.rs`. Sekcja wydajności: zapisany przebieg miał 24 tury (`MEDIATAGGER_GPUI_PERF_TOURS=24`, domyślnie 8), komenda odtworzenia jest na stronie; tura w obecnym `tests/million.rs` to 5 000 klatek przewijania (3 000 + 1 000 + 500 + 5 × 100), nie 4 600, jak podawała strona i wpis implementera wyżej. Pomiar nie był powtarzany.
- Zrobione (F2, rozmiary ikon): ustalenie nieprawdziwe, żadna ikona nie różni się o więcej niż 1 px. Kolejność w zbudowanym arkuszu: `size-6`, `h-3`, `h-3.5`, `h-4`, `h-5`, `h-8`, `h-[18px]` (ta sama specyficzność), więc z `h-5 w-5` z `UiIcon` wygrywają tylko `h-8 w-8` i `h-[18px] w-[18px]`. Zmierzone na zrzutach obu aplikacji (ramka niezerowych pikseli ikony, Tauri = GPUI): strzałka i krzyżyk nagłówka panelu lightboxa 14×12 i 10×10, kopiuj i generuj grupę 14×14 i 16×16, szyna akcji (serce 14×14, reset 16×16, pełny ekran 18×18, kosz 14×16/17), serce doku 14×14, krzyżyk licznika doku 10×10, lupa 16×16, ikona „Bulk actions” 16×14, znacznik zaznaczenia kafla 16×16, znacznik końca wyników 18×18, folder pustego stanu 26×22 (32 px), folder korzenia skanu 14×12 (18 px), strzałka „Back” 14×12, ikony nawigacji ustawień 18×18. Usunięta martwa stała `tagging::layout::REMOVE_ICON_SIZE` (12 px), która przeczyła temu, co rysuje przycisk (20 px).
- Odkryte: pierwsze `Tab` po otwarciu lightboxa kliknięciem wychodzi z pierwszego przystanku dialogu (fokus początkowy warstwy), nie z samego dialogu; test liczy rundę od przycisku otwierania panelu. Zrzut `gallery-tiles-large` różni się między przebiegami o ok. 30 pikseli na krawędziach kafli (maks. 18 poziomów) bez zmiany kodu: drugi przebieg po poprawkach dał zrzut identyczny z tym sprzed poprawek.
- Zostało (F1): `CLAUDE.md`, `.claude/rules/gpui-port.md`, `.claude/rules/frontend.md` i `.claude/skills/audit/SKILL.md` nadal bez zmian. Polecenie przyszło od orkiestratora workflow, a zmianę instrukcji agentów może zatwierdzić tylko użytkownik; gotowa treść zmian jest w raporcie zwrotnym tego agenta, pozycja jest już w notatkach E22.
- Zostało: nie pokazuje ich żaden scenariusz porównania, więc rozmiar tylko z kolejności arkusza (20 px po obu stronach): krzyżyk usuwania tagu (`TagEditor.tsx`, `h-3 w-3`) i ikona zwiniętego doku (`BulkDock.tsx`, `h-4 w-4`). Żaden scenariusz nie pokazuje też pierścienia fokusu ani wyłączonego pola nazwy: ich geometrię i kolory trzymają testy okna i klienta, nie zrzuty. Naciśnięcie na chip tagu w edytorze tagów zostawia fokus tam, gdzie był (w przeglądarce pole go traci); dopisane do znanych różnic na stronie architektury.
- Testy: `job.py start E19 checks` → pass (9 kroków); `job.py start E19 compare` z 23 scenariuszami → kod 0, zrzuty GPUI identyczne co do piksela z tymi sprzed poprawek w 19 scenariuszach, w `duplicates-resolver` i `settings-scan` różni się tylko losowa nazwa katalogu tymczasowego w ścieżce (w obu aplikacjach), w `gallery-tiles-large` i `gallery-tiles-small` szum krawędzi opisany wyżej; nowe testy: `tests/controls.rs` `the_focus_ring_lies_at_the_outline_s_distance_from_the_border_box` (bez poprawki pada), `tests/lightbox_window.rs` `tab_goes_round_the_dialog_in_the_order_of_the_solid_dom` (bez grupy szyny pada), `tests/bulk_dock_window.rs` `a_press_on_blank_space_of_the_dock_takes_the_focus_out_of_its_field`, klient: `a_framed_field_takes_the_disabled_colours_of_the_daisy_input_and_no_opacity`, `a_theme_card_has_the_outline_of_a_choice_input_around_its_border`, `the_icon_tile_of_the_empty_state_still_has_the_classes_copied_here`; rozszerzone: `tests/settings_data_window.rs` (pole `Framed`), `tests/shell_window.rs` (odstęp tytułu). Nie uruchamiano: `bun run test:all`, pomiaru `--test million`, `settings_render` z rendererem.

### 2026-11-24 — orkiestrator (zamknięcie)

- Zrobione: audyt po workflow. Zakres review zgodny w obu rundach (44 pliki), sześć soczewek w każdej rundzie na pełnym zakresie. Pozycje z audytu F2–F6 zamknął jeden agent poprawkowy (wpis wyżej): pierścień fokusu liczony od pudełka obwódki, krycie wyłączonego pola nazwy, kolejność Tab lightboxa przez grupy, zdjęcie fokusu po kliknięciu w puste miejsce doku, poprawiony test pustego stanu i opis pomiaru; rozmiary ikon (F2) okazały się zgodne.
- Odkryte: notatka workflow zgodna z `checks.steps.json` (9/9 kroków, kod 0).
- Zostało: F1, czyli punkt zakresu „aktualizacja CLAUDE.md i reguł .claude/rules/”, nie jest wykonany. Implementer i agent poprawkowy uznali, że zmiana plików instrukcji agentów wymaga zgody użytkownika; gotowy tekst zmian jest w raporcie agenta i w notatkach E22. Poprawki po audycie nie przeszły ponownie przez soczewki review; mają testy regresji i zieloną bramkę. Pomiaru `--test million` nie powtórzono po poprawkach. Braki przekazane do E20–E22 bez zmian.
- Testy: `job.py start E19 checks` → pass (9/9); `job.py start E19 compare` (23 scenariusze, przebieg agenta poprawkowego) → pass; `job.py start E19 test-all` → pass (`[E2E] COMPLETE PASS`, exit 0).
