# E05: Prymitywy UI i warstwa nakładek (GPUI)

Rozmiar: L. Poziom: standardowy. Zależy od: E03, E04.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Zestaw kontrolek i jedna warstwa modali, z których składają się wszystkie ekrany.

## Zakres

- Motyw jako global GPUI z tokenów klienta; rejestracja fontów Inter i JetBrains Mono z `public/fonts`.
- 22 ikony z `UiIcon.tsx` jako zasoby SVG.
- Kontrolki: przycisk (warianty), przycisk-ikona, chip, alert, pasek postępu, suwak, kontrolka segmentowa z przesuwanym podświetleniem, przełącznik, spinner z opóźnieniem, pole tekstowe z E04 ostylowane tokenami.
- Stos warstw i modal: Escape tylko dla górnej warstwy, pułapka fokusu i przywrócenie fokusu, wejście/wyjście wyłącznie przez opacity, modal zostaje zamontowany podczas wygaszania.
- Scenariusz `kit` (tylko profil `.gpui.e2e`): plansza wszystkich kontrolek w obu motywach.

## Poza zakresem

- Ekrany funkcji.

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/ui/controls`
- `src/ui/overlays`
- `src/ui/dom`
- `src/styles.css`

Testy do przeniesienia:
- `src/ui/controls/__tests__`
- `src/ui/overlays/__tests__`
- `src/ui/dom/__tests__`

Dokumenty i reguły:
- `docs/frontend/architecture-and-ui-conventions.md`
- `.claude/rules/frontend.md`

## Kryteria akceptacji

- Testy `#[gpui::test]`: kolejność Escape w stosie warstw, pułapka i przywrócenie fokusu, klawiatura suwaka i kontrolki segmentowej.
- Zrzut `kit` w obu motywach zgodny z tokenami (ocena agenta parytetu względem `src/styles.css`).
- Żadnego tekstu na sztywno; etykiety dostępności z i18n.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E05 checks`:
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

- Nie animować wymiarów ani layoutu (konwencja frontendu).

## Status

done 2026-11-21

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E04

- Z epiku E04: pole tekstowe to `media_tagger_gpui::ui::text_input::TextInput` (encja; model edycji w `media_tagger_client::text`, ADR 0038). Jest już ostylowane tokenami `input` z `src/styles.css` przez `text::field_visuals` (tło, obwódka i pierścień fokusu jako cienie, placeholder, zaznaczenie, przezroczystość 0,55 dla wyłączonego); `FieldChrome::Bare` zdejmuje ramkę (pole w edytorze tagów), `set_height` daje stałą wysokość (`h-8`, `h-10`). Motyw dostaje dziś w konstruktorze (`TextInput::new(theme, window, cx)`) i przez `set_theme`: global motywu z tego epiku powinien to zastąpić albo wołać `set_theme` przy zmianie.
- Z epiku E04: pole dziedziczy font, rozmiar i interlinię ze stylu tekstu rodzica (`window.text_style()`); bez rejestracji fontów rysuje fontem systemowym. Kursor nie mruga (stabilne zrzuty).
- Z epiku E04: `ui::init(cx)` rejestruje skróty klocków z `ui/` przy starcie (`window::run_until_closed`); nowe kontrolki dopisują tam swoje. Zasady skrótów z ADR 0038 obowiązują każdy skrót: nigdy bez kontekstu klawiszy, a skrót na klawiszu wpisującym znak (litera, cyfra, spacja, także z Shift) z kontekstem `text_input::outside_text_input(kontekst)`. Skrót bez kontekstu zarejestrowany po `ui::init` odebrałby polu Enter, Escape i Ctrl+A.
- Z epiku E04: Escape modala: pole wysyła `text_input::actions::Cancel` (i `Confirm` dla Enter), których samo nie obsługuje. Nieobsłużony przez właściciela Escape spada do następnego powiązania klawisza `escape` w płytszym kontekście, więc Escape górnej warstwy wystarczy powiązać w kontekście modala. Tab nie jest powiązany w polu; uchwyt fokusu pola jest przystankiem tabulacji (`tab_stop(true)`), wyłączone pole nim nie jest.

### 2026-11-21 — workflow gpui-port-epic
- Zrobione: cały epik E05. Crate klienta (bez GUI): `controls` (wygląd przycisku, przycisku ikonowego, chipa, alertu, paska postępu, suwaka, kontrolki segmentowej, przełącznika i spinnera wyliczony z tokenów motywu i klas kontrolek Solid; testy czytają `src/styles.css` i źródła `.tsx`), logika suwaka, strzałek segmentów, port `measureSegmentedHighlight`, opóźnione pokazanie spinnera, `overlay` (stos warstw, Escape tylko do górnej, fokus początkowy i jego przywracanie, stan wejścia i wyjścia modala z tokenami czasu, rozmiary i migawka wartości), `icons` (22 nazwy), `kit` (etykiety tablicy we wszystkich 11 językach), `CubicBezier::sample`. Crate GPUI: `ui::theme` (aktywny motyw jako global), `ui::fonts` (Inter i JetBrains Mono z `public/fonts`), `ui::assets` (22 ikony SVG w `assets/icons/`, test zgodności z `UiIcon.tsx`), kontrolki, `ui::layers` (Escape, Tab, pułapka fokusu, przywracanie fokusu), `ui::modal` (`ModalState` i `Modal`, zanikanie samą przezroczystością), `TextInput::themed`, scenariusz `--scenario kit` (tylko profil E2E). Dokumenty: konwencje frontendu, testing.md, system-overview.md, ADR 0039 z wpisem w indeksie, jedna linijka w `.claude/rules/frontend.md`, notatki w E06, E09, E16, E17. Agent testów dodał 19 testów `#[gpui::test]` (kontrolki, warstwy nienmodalne, Escape, Tab, reduced motion, zanikanie) i jeden getter `TextInput::theme()`.
- Odkryte: (ADR 0039) cała treść okna idzie w `ui::layers::root(cx)`, które wiąże Escape, Tab i Shift+Tab w kontekście `UiLayers`; okno musi wołać `ui::layers::focus_root` przy otwarciu i z `cx.on_focus_lost` (bez tego klawisze nie trafiają w żadne powiązanie, a `layers::forget` nie przesuwa fokusu), co dotyczy E06. Escape, który nic nie zamyka, propaguje dalej; nieobsłużony `Cancel` pola spada do Escape warstw. Tab w górnym modalu zawija się po przystankach modala. Decyzje o warstwach leżą w `media_tagger_client::overlay::LayerStack`, adapter trzyma uchwyty fokusu i wywołania zwrotne, a fokus początkowy i przywracanie idą przez `Window::on_next_frame` (testy: `window.simulate_next_frame(cx)`). Warstwa modala jest zwalniana na początku zamykania. `ModalMotion::LIGHTBOX` daje 75/50 ms; czas z zegara executora. Slider i SegmentedControl są kontrolowane (zdarzenia `SliderEvent::Input`, `SegmentedEvent::Change`), Button i Switch to elementy. `window::init_application(theme, cx)` rejestruje fonty, `ui::init` i motyw; `run_until_closed` przyjmuje motyw i `MainScreen`. `ui::init` wiąże też skróty warstw, suwaka i segmentów (test liczby powiązań z E04 je liczy). `kit` to stała `scenario::KIT` poza `SCENARIOS`, `epics.json` i `e2e/compare/scenarios`.
  - Odłożone: `bun run gpui:control start --scenario kit` i `compare:ui` nie uruchomią tablicy, bo `e2e/compare/gpui-control.js` przyjmuje tylko id z `e2e/compare/scenarios` (a `e2e/` było poza zakresem); przegląd przez test `kit` i `MEDIATAGGER_GPUI_SCREENSHOT_DIR` (`kit-headless.png`, `kit-dialog-headless.png`). Niezamodelowane: `inert`/`aria-hidden` tła i blokada przewijania body (modal przesłania wskaźnik i pułapkuje Tab). Klik klawiaturą w GPUI działa na Enter i Space dla segmentu i przełącznika (natywny radio/checkbox tylko na Space). Nieodtworzone: odstęp liter przycisku (`tracking-[-0.01em]`) i zmienna animacja kreski `loading-spinner`. Przy `App::reduce_motion` GPUI zamraża spinnery (CSS je kręci); nic dziś tego nie ustawia. Dla późniejszych epików: wirtualna lista z `createVirtualList` (E13), karty wyboru motywu i szewron `.theme-select` (E17), wariant `Modal` bez powierzchni dialogu dla lightboxa (E16), podpięcie `layers::root` i `focus_root` w powłoce (E06).
  - Pominięte przypadki testowe (powód): atrybuty DOM, klasy CSS, id aria, nazwy dostępne radio i grupy z `controls.test.tsx` (brak odpowiednika w GPUI, drzewo dostępności nieaktywne w oknie testowym; kształty ikon i style pokrywają testy klienta i test zasobów); odłączanie ResizeObservera (mechanika DOM, logika slajdu pokryta); w `UiModal.test.tsx`: blokada overflow body, inert/aria-hidden, odczyt tokenów przez getComputedStyle (tokeny sprawdza parytet motywu z E03), anulowanie animacji WAAPI i liczniki timerów, zmiana rozmiaru bez remontu (szerokości pokryte testem klienta), Escape bez providera (w GPUI zawsze jest global warstw); `theme.test.ts` (zrobione w E03); `createVirtualList.test.ts` (E13); `ThumbnailImage.test.tsx` (E09).
  - Przegląd: agent `review-gpui` nic nie zwrócił w rundzie 1 i 2 (luka pokrycia przeglądu); pozostałe soczewki zgłosiły 2 potwierdzone ustalenia, oba naprawione.
- Zostało: nic nie blokuje; brak otwartych, spornych, wcześniejszych ani niepewnych ustaleń przeglądu (raport nie powstał). Przegląd kitu na prawdziwym Xvfb przez harness porównawczy wymaga rozszerzenia `e2e/compare/gpui-control.js` w osobnej zmianie. Nie wykonano rzeczywistego uruchomienia binarki E2E ani testu z prawdziwą metodą wprowadzania.
- Testy:
  - `media_tagger_client` (348 testów jednostkowych i cele integracyjne): przechodzi.
  - `media_tagger_gpui` (wszystkie cele, w tym controls 11, overlays 19, text_input 21, kit): przechodzi.
  - `architecture_boundaries` (33): przechodzi.
  - `bun run gpui:lock-parity`: przechodzi.
  - `bun run test:compare` (50): przechodzi.
  - `cargo fmt --check` i clippy `-D warnings` (GPUI): przechodzi.
  - `bun run test:all` (bramka, runda 1): przechodzi.

### 2026-11-21 — naprawa luk (przegląd GPUI i ocena zrzutów kit)
- Zrobione: zastępczy przebieg soczewki `review-gpui` po całej zmianie E05 (luka rund 1 i 2) i niezależna ocena zrzutów `kit` w obu motywach; raport [reports/E05-review-open.md](reports/E05-review-open.md). Naprawione: gpui-1 (P2) podświetlenie segmentów rysowane w następnej klatce po pomiarze w prepaint, gpui-2 (P3) zamykany modal oddaje fokus korzeniowi, więc Enter i Space w czasie zanikania nie aktywują jego przycisków, gpui-3 (P3) przycisk z półprzezroczystym tłem bez zewnętrznego cienia (`ui::theme::shadows_beneath`). Dokumenty: konwencje frontendu i testing.md.
- Odkryte: `cx.notify()` w prepaint lub paint nie przerysowuje okna (`WindowInvalidator::invalidate_view` poza `DrawPhase::None`); stan zmieniony w fazie rysowania potrzebuje `window.on_next_frame` z `cx.notify(id)` (src-gpui/crates/media_tagger_gpui/src/ui/segmented.rs:241). GPUI rysuje zewnętrzny cień pod całym elementem (src-gpui/crates/media_tagger_gpui/src/ui/theme.rs:75); dotyczy też późniejszych kontrolek z półprzezroczystym tłem i cieniem.
- Zostało: nic otwartego. Resztka gpui-2: Tab w ciągu 100 ms zanikania może trafić na przycisk zamykanego dialogu. Pierścień fokusu oceniony w kodzie, nie na zrzucie.
- Testy:
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test overlays` → pass (20)
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test controls` → pass (12)
  - `MEDIATAGGER_GPUI_REQUIRE_RENDERER=1 cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test kit --lib` → pass
  - `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check` i clippy `-D warnings` → pass
  - `job.py start E05 checks` (rustfmt, clippy obu workspace'ów, `architecture_boundaries`, `gpui:lock-parity`, testy `media_tagger_client` i `media_tagger_gpui`) → pass
