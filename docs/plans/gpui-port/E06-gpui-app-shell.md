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

done 2026-11-21

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-20 — implementer E00

- Z epiku E00: `window.rs` otwiera okno z domyślnymi dekoracjami serwera i widokiem-zaślepką `views/hello.rs` (model `media_tagger_client::hello`); dekoracje po stronie klienta (D14) i właściwa powłoka należą do tego epiku, a `hello` można wtedy usunąć razem z testami `tests/hello_window.rs`. Rozmiar 1440×900 i minimum 1000×720 są przeniesione z `src-tauri/tauri.conf.json`; tytuł i `app_id` pochodzą z profilu (`profile.rs`).
- Z epiku E00: `Boot` (AppState + blokada instancji) żyje w `media_tagger_gpui::run` do powrotu z pętli zdarzeń; `startup_scan_roots` czeka na single-flight skanu startowego.

### 2026-11-21 — implementer E04

- Z epiku E04: rejestr skrótów musi trzymać zasady z ADR 0038 (`docs/frontend/architecture-and-ui-conventions.md`, „GPUI port: keys and text fields”): każdy skrót w kontekście klawiszy powłoki albo głębszym, nigdy bez kontekstu; skróty na klawiszach wpisujących znak przez `text_input::outside_text_input(kontekst)`. Ctrl+K wiązany w kontekście powłoki działa także z fokusem w polu (pole go nie używa), jak w Tauri. Wzorzec testu: `tests/text_input.rs` (`shell_bindings`).
- Z epiku E04: wyłączenie pola, które ma fokus (`TextInput::set_disabled`), zdejmuje fokus z okna; bez sfokusowanego elementu w ścieżce nie ma żadnego kontekstu klawiszy i skróty powłoki nie pasują. Powłoka powinna odzyskać fokus w `cx.on_focus_lost(window, …)` (tak robi Workspace w Zed) i mieć własny `FocusHandle` na korzeniu.
- Z epiku E04: `ui::init(cx)` jest już wołane w `window::run_until_closed` przed otwarciem okna; testy na `TestAppContext` wołają je same.

### 2026-11-21 — implementer E05

- Z epiku E05: korzeniem treści każdego okna musi być `ui::layers::root(cx)` (kontekst klawiszy `UiLayers` z Escape, Tab i Shift+Tab oraz ich obsługą, ADR 0039); okno przy otwarciu i w `cx.on_focus_lost(window, …)` woła `ui::layers::focus_root`, bo bez sfokusowanego elementu GPUI wysyła klawisze do własnego węzła nad wszystkimi elementami i żadne powiązanie aplikacji nie pasuje. Wzorzec: `views/kit.rs`. Styl korzenia (Inter, 13 px, interlinia 1,5, `--bg-1`): `ui::theme::app_root(element, motyw)`.
- Z epiku E05: `window::init_application(motyw, cx)` rejestruje fonty, woła `ui::init` i ustawia globalny motyw (`ui::theme::set_active`); `run_until_closed` przyjmuje motyw i `MainScreen`. Zmiana motywu z ustawień (E17) to `ui::theme::set_active`; kontrolki czytają global, pole tekstowe tylko gdy powstało przez `TextInput::themed`.
- Z epiku E05: warstwy niemodalne (strona ustawień, tryb bulk) rejestruje się przez `ui::layers::register(LayerOptions::nonmodal(..), ..)` i zwalnia przez `ui::layers::release(id, cel_przywrócenia, ..)`; samodzielne obsługi Escape są zabronione. Tab bez modala to `Window::focus_next` (powiązany w `UiLayers`, nie w powłoce).

### 2026-11-21 — implementer E06 (parity, runda 1)

- Z epiku E06: backend X11 GPUI tworzy każde okno o 2 px na prawo od żądanego początku (`gpui_linux`, `X11Window::new`), więc okno szerokie jak ekran zostawiało 2 px czarnego tła roota przy lewej krawędzi. `window::main_window_bounds` odejmuje to przesunięcie, gdy `cx.compositor_name()` to `"X11"`; na Waylandzie granice zostają wyśrodkowane. Przesunięcie jest w pikselach urządzenia, a skala nie jest znana przed oknem, więc na skalowanym ekranie X11 korekta jest o piksel lub dwa przybliżona. Przy podbiciu rewizji Zed sprawdzić, czy obejście w GPUI nadal istnieje.

### 2026-11-21 — workflow gpui-port-epic

- Zrobione: Runtime aplikacji, okno i powłoka w obu crate'ach portu. Klient (`media_tagger_client`): `runtime.rs` z `runtime/tests.rs` (`AppRuntime` jako maszyna stanów odpowiednik `createAppRuntime`: widok galeria/ustawienia, podświetlenie sekcji skanu, motyw, język, rozmiar kafelka, łączna liczba, katalogi główne, faza skanu startowego; `RuntimeRunner` wykonuje efekty przez port `Backend` i `PrefsStore`), `window_chrome.rs` (przyciski okna, 8 uchwytów zmiany rozmiaru, gest przeciągania), `shell.rs` (miary nagłówka, tekst liczby elementów, wybór stanu pustego, `PRODUCT_NAME`), `shortcuts.rs` (konteksty Shell, Gallery, Settings i pusty rejestr skrótów); usunięto `hello.rs`. Adapter GPUI: `runtime.rs` (encja `Runtime`, `WakeSignal`), `views/shell.rs`, `views/gallery.rs`, `views/settings.rs`, `views/chrome.rs`, `window.rs` (dekoracje po stronie klienta), `lib.rs`, `boot.rs` (`Arc<AppState>`), `ui/logo.rs`, `Button::leading_icon`; usunięto widok `hello` i `tests/hello_window.rs`. Testy: 26 nowych zachowań (13 w `tests/app_runtime.rs`, 13 w `tests/shell_ported.rs`), do tego `tests/shell_window.rs` (10) i przepisany `tests/scenario_window.rs`. Dokumentacja: `docs/frontend/architecture-and-ui-conventions.md`, `docs/architecture/system-overview.md`, `docs/development/testing.md`.
- Odkryte:
  - Wywołania backendu idą przez `ThreadExecutor` z E02, nie przez pulę GPUI (blokujące skany zajęłyby stałą pulę); odpowiedzi budzą UI przez `WakeSignal`. Encję budować z `RuntimeParts { config, backend, prefs, executor }`.
  - Kolejne modele (E07 zapytania, E11 zaznaczenie, E15 lightbox, E17 ustawienia) mają leżeć obok `AppRuntime` w `media_tagger_client::runtime` albo jako encje rodzeństwa w `ShellView`; widoki są tworzone na nowo przy każdej zmianie widoku, więc nic trwałego nie może w nich mieszkać.
  - Do E07 odświeżenie biblioteki to jedno niefiltrowane `start_asset_query` z właścicielem `gpui-app-runtime` (tylko dla `total`). E07 ma je zastąpić swoim modelem, zachowując `RuntimeInput::RefreshLibrary` i regułę kończenia skanu startowego (po odpowiedzi obu odświeżeń). Stały właściciel `LIBRARY_OWNER_ID` znaczy, że drugi `AppRuntime` na tym samym rdzeniu dostaje `Superseded`; to zgodne z zasadą jednego runtime'u.
  - Konteksty klawiszy: `Shell` na korzeniu okna razem z `UiLayers` (`views::shell::shell_key_context()`), `Gallery` i `Settings` na korzeniach widoków. Powłoka nie ma własnego `FocusHandle` (liczyłby się jako zachowany fokus i zatrzymał przywracanie fokusu przez stos warstw); fokus wraca do `layers::focus_root` przy otwarciu i w `on_focus_lost`.
  - `shortcuts::SHORTCUTS` jest pusty, bo powłoka Tauri nie wiąże klawiszy poza Escape. E12 dodaje Ctrl+K, kolejne epiki swoje, każdy z kontekstem; klawisze wpisujące znak (`types_a_character`) przez `text_input::outside_text_input`.
  - Strona ustawień to warstwa niemodalna rejestrowana i zwalniana przez `ShellView` (`layers::release(.., Some(settings_opener))`); uchwyt fokusu przycisku ustawień należy do powłoki.
  - Dekoracje: `WindowDecorations::Client`, własne przyciski zawsze rysowane, także gdy platforma wraca do dekoracji serwera (X11 bez kompozytora, w tym Xvfb), więc menedżer okien może narysować drugi pasek tytułu. Uchwyty ukryte tylko przy maksymalizacji i pełnym ekranie; przeciąganie startuje przy ruchu wskaźnika. Interaktywne dzieci powierzchni przeciągania muszą zatrzymać propagację mouse-down (`views::chrome::no_drag`).
  - Scenariusze startują z motywem scenariusza i angielskim; zwykły start z zapisanych preferencji, potem ciemny i język systemu; motyw i język startowy zapisywane przy `Start`, jak w bootstrapie Solid.
  - `Boot.state` jest `Arc<AppState>`, więc pula bazy zamyka się z procesem, po zwolnieniu blokady instancji nie przed nią.
  - Góra ciała galerii `pt` 20 (`shell::GALLERY_PADDING_TOP`); stan pusty wyśrodkowany w pudle o wysokości okna minus 180.
  - Odroczone zakresy: puste miejsca nagłówka (pole wyszukiwania z Ctrl+K, segmenty rodzaju, ulubione, przycisk listy tagów: E12, E13; przełącznik Bulk actions: E11, E14); siatka galerii i zoom Ctrl+kółko (`RuntimeInput::Zoom` istnieje, żaden widok go nie wysyła; E09); sekcje ustawień, w tym motyw i język (E17, E18; przełączanie na żywo działa przez `RuntimeInput::SetTheme`/`SetLanguage`).
  - Pominięte przypadki testów Tauri (powód): testy harnessu Solid w `bootstrap.test.tsx` (3, bez zachowania produktu); `shell.test.tsx`: wariant bulk (E11/E14), subskrypcja języka i18next, natywna synchronizacja motywu, połowa montażu/odmontowania DOM, instrumentacja wydajności, timer prefetch, lazy-chunki i błędy lazy lightboxa (6 przypadków, brak odpowiednika w GPUI); `gallery.test.tsx`: część wyszukiwania i rodzaju (E12/E13), błąd pojemności i zawężanie (E07/E12); `lightboxRecovery`, `rangeSelectionKeepalive` (7), `runtimeIntegration` (4): epiki lightboxa, zaznaczenia, bulk, ustawień; `startupScanRemount`: mechanika odmontowania Solid (zachowanie przeportowane); `preferenceStorage`: usterka getter-a localStorage; `WindowControls.test.tsx`: wyścigi asynchronicznego IPC (2), tryb podglądu przeglądarki, połowa maksymalizacji (testowe okno GPUI zawsze zgłasza brak maksymalizacji; pokryta testami jednostkowymi `window_chrome`).
  - Parity (11 drobnych różnic otwartych, wszystkie minor, scenariusze shell-empty i shell-empty-light): brak pola wyszukiwania (E12), segmentów rodzaju i ulubionych (E12), przycisku listy tagów z dzielnikiem (E13) i Bulk actions (E11/E14), przez co pigułka liczby stoi przy x≈16 zamiast ≈343; inny, szerszy krój pisma niż Inter (rejestracja fontów przy starcie, E06 boot lub E03 theme); środek stanu pustego o około 7 px w prawo (rezerwa na pasek przewijania, E09/E10); brak odstępów liter w logotypie i brak zmiany koloru ikony Back po najechaniu.
  - Zadania spoza uprawnień agentów: `CLAUDE.md` nadal wymienia usunięty cel `hello_window` w komentarzu `bun run test:gpui`, a `.claude/rules/gpui-port.md` mogłoby wymienić `runtime.rs`, `shell.rs`, `window_chrome.rs`, `shortcuts.rs`; użytkownik musi je poprawić.
- Zostało: Epik nie jest zablokowany, brak nierozwiązanych porażek i otwartych znalezisk przeglądu (przegląd czysty po 5 rundach: 5 potwierdzonych, 3 naprawione, 0 odrzuconych; brak raportu). Nie sprawdzono: `bun run compare:ui shell-empty shell-empty-light` na prawdziwym Xvfb (porównano tylko rendery headless), prawdziwego binarium na ekranie, dekoracji po stronie klienta na Waylandzie i X11 (ryzyko epiku) oraz realnych przesunięć i zmian rozmiaru przez menedżer okien. Brama test:all: zaliczona w 1 rundzie bez poprawek.
- Testy:
  - `cargo test --locked` `media_tagger_client` (371 testów jednostkowych, `app_runtime` 13): pass
  - `cargo test --locked` `media_tagger_gpui` (`shell_ported` 13/13, `shell_window` 10/10, `scenario_window` 3/3): pass
  - `architecture_boundaries` 33/33: pass
  - `bun run gpui:lock-parity`: pass
  - `cargo clippy --all-targets --all-features --locked -D warnings` (src-gpui) i `cargo fmt --check`: pass
  - `git diff --check`: pass
  - check/fix loop: pass (0 poprawek)
  - test:all (brama): pass
  - `compare:ui` na prawdziwym ekranie: nie uruchomiono

### 2026-11-21 — agent naprawczy luk E06

- Zrobione: sprostowanie wpisu „workflow gpui-port-epic” powyżej (wcześniejsze wpisy zostają bez zmian): (1) `CLAUDE.md` zostało poprawione przez agenta naprawczego rundy 4 przeglądu: komentarz `bun run test:gpui` wymienia teraz `shell_window`/`shell_ported` zamiast usuniętego `hello_window`; nie ma tu zadania dla użytkownika (zostaje tylko ewentualne uzupełnienie `.claude/rules/gpui-port.md`). (2) `compare:ui shell-empty shell-empty-light` uruchomiono na prawdziwym Xvfb przez `job.py` trzykrotnie (3 rundy parytetu, nie tylko rendery headless); dwie wyraźne różnice zostały nim znalezione i naprawione: 2 px pas tła przy lewej krawędzi okna na X11 i podtytuł stanu pustego 14 px zamiast 13 px. (3) „5 potwierdzonych, 3 naprawione”: pozostałe dwa potwierdzenia to to samo ustalenie o `CLAUDE.md` zgłoszone ponownie w rundach 2–4 i naprawione w rundzie 4; runda 5 była czysta. Własna praca: zbadana luka „inny krój pisma” z kryterium parytetu, test strażnik `the_window_text_uses_the_bundled_fonts` (src-gpui/crates/media_tagger_gpui/tests/shell_window.rs:517), raport `reports/E06-review-open.md`, zdanie o ograniczeniach fontów GPUI w `docs/frontend/architecture-and-ui-conventions.md`, wiersz testu w `docs/development/testing.md`.
- Odkryte:
  - Okno GPUI rysuje Inter: `init_application` rejestruje fonty na każdej ścieżce startu (src-gpui/crates/media_tagger_gpui/src/window.rs:282), korzeń okna ma `TYPOGRAPHY.sans[0]` (src-gpui/crates/media_tagger_gpui/src/ui/theme.rs:93), a rodzina rozwiązuje się do twarzy Inter (podtytuł 13 px: 351,67 px w GPUI, 351,66 px wg HarfBuzz dla `public/fonts/Inter.ttf`). Zrzut Tauri to też Inter, ale z przesunięciami zaokrąglonymi przez hinting WebKitGTK (358 px atramentu), a swash z hintingiem rysuje glify wyższe i węższe. To rasteryzacja, poza parytetem wg D10 (README.md:82, :186); kodu nie zmieniono.
  - GPUI przypiętego commitu nie stosuje osi wariacji: zmienny `Inter.ttf` daje tylko wagę 400; inne wagi biorą statyczne twarze Inter z systemu, jeśli są, a bez nich nagłówki, logotyp i przyciski rysują się cienko (pomiar z fontconfig bez systemowego Inter: 224,11 px dla wag 400–700). GPUI nie ma też `letter_spacing`, więc tokeny odstępów liter nie działają (nagłówek stanu pustego 229 zamiast 223 px). Obie rzeczy wymagają decyzji planu (statyczne instancje Inter jako zasoby albo zmiana GPUI); opis w raporcie, notatka w E22.
- Zostało: font-2 (P2) i font-3 (P3) z `reports/E06-review-open.md` do decyzji; różnice parytetu E12 (wyszukiwanie, rodzaj, ulubione), E13 (lista tagów), E11/E14 (Bulk actions), E09/E10 (rezerwa paska przewijania, środek stanu pustego 7 px w prawo). Ustaleń należących do E06: brak; P0 i P1: brak. Nie sprawdzono: wyglądu na Waylandzie i na systemie bez zainstalowanego Inter w prawdziwym oknie (tylko pomiar układu w systemie tekstu).
- Testy:
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test shell_window` (z `MEDIATAGGER_GPUI_REQUIRE_RENDERER=1`): pass, 12/12; nowy test bez `ui::fonts::register` w `init_application`: fail (JetBrains Mono → „Adwaita Sans”), po przywróceniu: pass
  - `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`: pass
  - `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`: pass
  - `job.py start E06 compare --scenario shell-empty --scenario shell-empty-light`: pass (kod 0), zrzuty ocenione pomiarami
  - `job.py start E06 checks`: pass (8/8 kroków)
