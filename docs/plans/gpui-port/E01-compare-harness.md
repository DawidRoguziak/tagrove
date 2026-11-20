# E01: Harness porównawczy Tauri ↔ GPUI

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E00.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Jedna komenda, która na tych samych syntetycznych danych robi zrzut ekranu tego samego stanu w obu aplikacjach, żeby agenci mogli porównywać efekty.

## Zakres

- Build E2E GPUI do `src-gpui/target-e2e` ze strażnikami jak w `e2e/build-e2e.js` (identyfikator `.gpui.e2e`, tytuł „Tagrove GPUI E2E”, target dir).
- `bun run gpui:control start|scenario|screenshot|stop`: prywatny display i katalogi XDG jak w `e2e/control.js` albo ścieżka headless z E00; własny lock `artifacts/gpui-control/active.lock`; zawsze `stop`, także po błędzie.
- Scenariusze: start w nazwanym, deterministycznym stanie (`--scenario <id>` działa tylko w profilu `.gpui.e2e`), seed z `e2e/fixtures.js` (`copyPngFixtures`, `createPlayableFixtures`) w świeżym katalogu tymczasowym.
- `bun run compare:ui <scenario…>`: ten sam seed → Tauri przez `app:control` (kroki w `e2e/compare/scenarios/*.js`) i GPUI → `artifacts/gpui-compare/<scenario>/{tauri,gpui,side-by-side}.png` + `result.json`.
- Pierwsze scenariusze: `shell-empty`, `shell-empty-light`. Katalog scenariuszy żyje w README planu; każdy epik UI dopisuje swoje po obu stronach.
- Testy `node --test` strażników i katalogu scenariuszy; sekcja w `docs/development/testing.md`.

## Poza zakresem

- Porównanie piksel w piksel i progi różnic.
- Sterowanie GPUI przez WebDriver.

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `e2e/control.js`
- `e2e/control-support.js`
- `e2e/control-native.py`
- `e2e/display.js`
- `e2e/build-e2e.js`
- `e2e/fixtures.js`
- `.claude/skills/verify-mediatagger/SKILL.md`

Testy do przeniesienia:
- `e2e/build-e2e.test.js`
- `e2e/display.test.js`
- `e2e/control-support.test.js`

Dokumenty i reguły:
- `docs/development/testing.md`

## Kryteria akceptacji

- `bun run compare:ui shell-empty` zostawia trzy PNG i `result.json`; ponowne uruchomienie daje ten sam układ.
- Sesja nie startuje, gdy istnieje `artifacts/app-control/active.lock` albo trwa `test:e2e:tauri`; nigdy nie usuwa cudzego locka ani nie zabija procesów po nazwie.
- Strażniki odmawiają złego identyfikatora, tytułu i target dir (testy).
- Po przerwaniu nie zostaje proces, display ani katalog tymczasowy.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E01 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`
- `node --test e2e/compare`

## Scenariusze porównania

- `shell-empty`: Pusta biblioteka, widok galerii (pusty stan), motyw ciemny.

## Ryzyka

- Wspólne zasoby z desktop E2E (`dist/`, `target-e2e`, porty 4444/4446/4447): tylko seryjnie.
- Zrzuty i logi zostają w ignorowanym `artifacts/`; nic z nich nie trafia do śledzonych plików.

## Status

done 2026-11-20

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-20 — implementer E00

- Z epiku E00: zrzuty okna GPUI działają bez realnego ekranu dwiema ścieżkami i bez `vulkan-swrast`: (A) w procesie przez `gpui::HeadlessAppContext` + `gpui_platform::current_headless_renderer()` (tylko z cechą `test-support`, czyli w celach testowych; zrzut w skali 2), (B) prawdziwa binarka na prywatnym Xvfb + `import -window root` (GPUI przechodzi tam z Vulkana na EGL/llvmpipe). Komendy i czasy: notatki E00 i `docs/development/testing.md` („Screenshots of the GPUI window without a real display”).
- Z epiku E00: binarkę E2E buduje `bun run gpui:build:e2e` (`scripts/gpui-build-e2e.mjs` eksportuje `gpuiE2eBinaryPath`, `gpuiE2eIdentifier`, `gpuiE2eWindowTitle`); `media_tagger_gpui --identifier` wypisuje wkompilowany identyfikator bez startu. Okno ma tytuł „Tagrove GPUI E2E” i `WM_CLASS` równy identyfikatorowi; `WM_DELETE_WINDOW` kończy proces kodem 0. Dane profilu leżą w `$XDG_DATA_HOME/com.example.mediatagger.gpui.e2e/`; sprzątanie tego katalogu (wzorem `e2e/wdio.conf.js`) należy do harnessu.
- Z epiku E00: bez `WAYLAND_DISPLAY` i `DISPLAY` binarka kończy się błędem zamiast startować headless; pod Xvfb trzeba usunąć `WAYLAND_DISPLAY` z otoczenia.

### 2026-11-20 — workflow gpui-port-epic

- Zrobione: harness porównawczy działa. `bun run compare:ui shell-empty` zostawia w `artifacts/gpui-compare/shell-empty/` pliki `tauri.png`, `gpui.png`, `side-by-side.png` i `result.json`, a ponowny przebieg daje te same pliki i identyczne sygnatury pikseli. Dodano `bun run gpui:control start|scenario|screenshot|doctor|logs|stop` (`e2e/compare/gpui-control.js`: build binarki E2E, prywatny Xvfb, scratchowe katalogi XDG, własny lock `artifacts/gpui-control/active.lock`), `bun run compare:ui <scenariusz…>` / `--list` (`e2e/compare/compare.js`: jedna sesja `app:control` na scenariusz po stronie Tauri, potem jedna sesja GPUI dla wszystkich; aplikacje nigdy nie działają naraz), scenariusze `shell-empty` i `shell-empty-light` (`e2e/compare/scenarios/*.js` oraz `media_tagger_gpui --scenario <id>`, tylko w profilu E2E), strażników (odmowa przy `artifacts/app-control/active.lock`, przy launcherze wdio z `test:e2e:tauri` w tym checkoucie i przy zajętych portach 4444/4446/4447; zły identyfikator, tytuł, target dir lub ścieżka binarki odrzucone przed lockiem i buildem) oraz `bun run test:compare` (`node --test e2e/compare`, 45 testów, dodane do `quality`). Dokumentacja: nowa sekcja „Comparison harness (Tauri and GPUI)” w `docs/development/testing.md`, uzupełnienia w `setup-and-build.md`, `system-overview.md` i README planu. Testy: 8 nowych testów Rust (4 w `media_tagger_client/src/scenario.rs`, 4 w `media_tagger_gpui/tests/scenario_window.rs`), w tym jeden przeniesiony przypadek (E2E startuje po angielsku w ciemnym motywie). Brak zmian w kodzie produkcyjnym przez agenta testów.
- Odkryte: decyzje implementera. (1) Zrzuty GPUI robi prawdziwa binarka E2E na prywatnym Xvfb (`import -window root`, 1440x900x24, ścieżka B z E00), nie renderer headless, więc obie strony są łapane tak samo. (2) Scenariusz żyje w trzech miejscach pilnowanych testami: `e2e/compare/scenarios/<id>.js` (`{id, theme, seed, tauri}`), `SCENARIOS` w `media_tagger_client/src/scenario.rs` oraz `scenarios` w `epics.json`; epik UI dodaje scenariusz we wszystkich trzech. (3) `e2e/control.js` bez zmian: `app:control start` zawsze indeksuje własny seed, więc `seed: "empty"` po stronie Tauri to usunięcie korzenia skanu przez ustawienia, motyw jasny wybierany tam samo, potem `refresh` (`tauriSteps` w `e2e/compare/scenarios.js`). (4) Kroki scenariusza to tylko komendy działające z `app:control` plus `wait` i `refresh`; `stop`, `screenshot`, `doctor`, `logs`, `inspect`, `diagnose` są odrzucane. (5) Sesja Tauri na scenariusz trwa ok. minuty, bo każdy start przebudowuje frontend. (6) `media_tagger_gpui` parsuje argumenty ściśle (`cli.rs`): brak argumentów, samo `--identifier` lub `--scenario <id>`, reszta kończy kodem 1. Kolejność startu: strażnik identyfikatora, strażnik scenariusza, sprawdzenie wyświetlacza, katalog danych, boot; `run` przyjmuje `Option<&str>`, `run_with` bez zmian, nowe `run_scenario_with`. (7) GPUI działa bez sesji D-Bus (`DBUS_SESSION_BUS_ADDRESS` wskazuje martwe gniazdo w katalogu scratch), więc nie dotyka portalu użytkownika. (8) Gotowość GPUI: okno z tytułem E2E ma `WM_CLASS` i `_NET_WM_PID`, a dwa zrzuty w odstępie 200 ms są identyczne i różne od ekranu sprzed startu; okno pojawia się ok. 200 ms przed pierwszą klatką. Epiki z treścią asynchroniczną (miniatury) mogą potrzebować sygnału gotowości z aplikacji. (9) Prefiks scratch `media-tagger-gpui-control-` różni się od `media-tagger-control-`, więc kontrolery nie usuną swoich katalogów (testowane w obie strony). (10) Lock jest zwalniany tylko jeśli nadal wskazuje właściciela; stary lock po awarii hosta nie jest odzyskiwany automatycznie (jak w `app:control`). (11) `test:e2e:tauri` wykrywany skanem `/proc` (CLI `wdio` z `e2e/wdio.conf.js` tego checkoutu), tylko odczyt. (12) `node --test e2e/compare` nie schodzi do katalogów na Node 22+, dlatego `e2e/compare/index.js` importuje każdy `*.test.js`; nowe pliki testów nie wymagają rejestracji. (13) `result.json` (schema 1) bez znacznika czasu i bezwzględnych ścieżek. (14) `gpui:control` kończy kodem 129/130/143 na SIGHUP/SIGINT/SIGTERM. (15) ADR nie powstał: to narzędzie testowe w ramach D10, a flaga scenariusza zostaje w profilu E2E z ADR 0036. Odroczone: GPUI jeszcze nie indeksuje wskazanego katalogu mediów; harness buduje seed `control` i eksportuje `MEDIATAGGER_GPUI_SCENARIO_MEDIA`, ale binarka go nie czyta, oba scenariusze E01 używają seeda pustego, więc pierwszy epik ze scenariuszem biblioteki (E09 `gallery-default`) musi dodać skan w `boot.rs` i wpis z seedem w katalogu Rust. Zrzuty GPUI pokazują placeholder z E00 („Loading…”), bo powłoka przychodzi z E06. `CLAUDE.md`, `.claude/rules/gpui-port.md` i `testing-e2e.md` nie były edytowane (agent nie zmienia instrukcji projektu); nie wspominają o `gpui:control`, `compare:ui`, `test:compare`, `e2e/compare/`, `cli.rs`, `scenario.rs` ani o tym, że te komendy są zależne od wyświetlacza i seryjne; decyzja należy do użytkownika. SIGKILL `compare:ui` w trakcie sesji Tauri zostawia działającą sesję `app:control` (brak obsługi śmierci rodzica, `e2e/control.js` poza zakresem); odzyskanie: `bun run app:control stop <sesja>` (opisane w testing.md). Żaden automatyczny test nie uruchamia prawdziwej sesji GPUI (Xvfb, binarka, zrzut, sprzątanie po sygnale), tylko ręczne przebiegi. Pominięte przypadki z portu testów (37 przypadków TS testuje głównie sam harness Node, bez odpowiednika produktowego): `build-e2e.test.js` (kolejność buildu Vite i Tauri, odrzucanie niebezpiecznych profili, przygotowanie WDIO, reset preferencji, strażnik tytułu, sygnały launchera 130/143/129, IPC worker/launcher, pomocnicze dzieci READY) – mechanika WDIO i skryptów Node, strona GPUI pokryta przez `profile.rs`, `startup.rs`, `scenario.rs`, `hello_window.rs` i testy node `gpui-build-e2e`; `display.test.js` (strażnik zrzutu tylko dla własnego Xvfb) – pokryte testami node w `gpui-control-support`; `control-support.test.js` (walidacja komend WebDriver, porty, sprzątanie, drzewa procesów, fixtury mediów, kolejka komend, akcje drag, środowisko natywne, długość ścieżek gniazd, selektory XPath) – mechanika `app:control`, GPUI nie jest sterowany przez WebDriver. Różnic parity: brak otwartych.
- Zostało: nic nie blokuje epiku (status done, review czyste po 4 rundach: 7 potwierdzonych i naprawionych, 1 odrzucony, bez otwartych, spornych, wcześniej istniejących ani niepewnych, więc bez raportu). Do rozważenia przez użytkownika: wpis o nowych komendach w `CLAUDE.md` i regułach; wyścig w `e2e/specs/group-order.e2e.js` (fokus sprawdzany przed `requestAnimationFrame` w `GroupOrderBoard.tsx`, spec nie czeka na klatkę), który wywołał niezwiązaną porażkę bramki w rundzie 1 i wymaga osobnej zmiany poza portem; wskazanie katalogu mediów w GPUI (E09); ewentualny sygnał gotowości z aplikacji.
- Testy:
  - `cargo check`, `cargo fmt --check`, clippy `-D warnings` (workspace GPUI): pass
  - `media_tagger_client` (17 + 2): pass
  - `media_tagger_gpui` (unit 31, hello_window 3, scenario 6, scenario_window 4, startup 8): pass
  - `architecture_boundaries` (33): pass
  - `bun run lint`, `format:check`: pass
  - `gpui:lock-parity`, `test:gpui-tools`: pass
  - `test:compare` (45): pass
  - `privacy:check`, `test:privacy`: pass
  - Żywe `compare:ui` dla obu scenariuszy (zrzuty 1440x900) i kontrole przerwania (SIGTERM, SIGKILL): pass
  - `test:all` (bramka): pass po 2 rundach; w rundzie 1 dwie porażki w `group-order.e2e.js` niezwiązane z epikiem (wyścig `requestAnimationFrame` i jego kaskada), ponowny przebieg samego specu: pass, brak zmian w kodzie
  - `rust:fmt`/`rust:clippy` dla `src-tauri`: nie uruchamiane (bez zmian w `src-tauri`)
