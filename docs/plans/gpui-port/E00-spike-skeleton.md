# E00: Spike stosu i osobny workspace GPUI

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: —.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Postawić osobny workspace `src-gpui/` z GPUI wziętym wprost z repozytorium Zed, uruchomić rdzeń bez Tauri w oknie GPUI i ustalić, jak robić zrzuty ekranu bez realnego ekranu. Workspace Tauri i jego `Cargo.lock` zostają nietknięte.

## Zakres

- Osobny workspace `src-gpui/` (D2): `Cargo.toml` z `[workspace]`, własny `Cargo.lock`, crate'y `crates/media_tagger_client` (lib, bez GUI) i `crates/media_tagger_gpui` (bin `media_tagger_gpui`); rdzeń jako zależność ścieżkowa `../src-tauri/crates/media_tagger_core`. `src-tauri/Cargo.toml` i `src-tauri/Cargo.lock` nie zmieniają się.
- Pierwszy krok: potwierdzić, że `*.workspace = true` w manifeście rdzenia rozwiązuje się z `src-tauri/Cargo.toml`, gdy rdzeń jest zależnością ścieżkową innego workspace. Jeśli nie, zatrzymać się i zgłosić `blocked` z opisem (zmiana manifestu rdzenia to decyzja użytkownika).
- GPUI z repozytorium Zed (D1): zależność git `https://github.com/zed-industries/zed` przypięta `rev = "<pełny SHA>"` commita, na który wskazuje tag stabilnego wydania Zed; wszystkie crate'y brane z Zed (`gpui` i te, których wymaga uruchomienie aplikacji) z tego samego `rev`. Jeśli build wymaga wpisów z sekcji `[patch]` Zed, powtórzyć tylko konieczne i opisać każdy.
- **Kontrola pochodzenia przed pierwszym buildem:** `rev` należy do `zed-industries/zed` i jest osiągalny z tagu wydania; jedyne źródła git w `src-gpui/Cargo.lock` to Zed oraz forki, które Zed sam przypina; lista pakietów, których nie ma w `src-tauri/Cargo.lock`, z licencjami; `cargo audit --file src-gpui/Cargo.lock`. Pierwszy build nowych zależności w piaskownicy `bwrap` (bez sieci po `cargo fetch`, bez dostępu do katalogu domowego poza repozytorium i katalogami cargo).
- Parytet rdzenia między lockami: `bun run gpui:lock-parity` porównuje wersje wszystkich pakietów z domknięcia zależności `media_tagger_core` w `src-tauri/Cargo.lock` i `src-gpui/Cargo.lock` i pada przy różnicy (ta sama bundlowana SQLite w obu aplikacjach, ADR 0032). Skrypt ma własny test.
- Build na toolchainie z `rust-toolchain.toml`. Jeśli przypięty commit Zed wymaga nowszego, zgłosić `blocked`: zmiana toolchainu dotyczy też Tauri.
- Okno „hello”: tytuł profilu, tło z tokenu motywu, jeden napis z `src/i18n/locales/en.json` wczytanego przez `include_str!`.
- Profile `.gpui.dev` / `.gpui.e2e` (D5): strażnik identyfikatora w buildzie debug, katalog danych, `InstanceLock`, `bootstrap::open_storage`, `resolve_ffmpeg_path`, `start_workers`; zamknięcie okna kończy proces czysto.
- Spike zrzutów bez ekranu, w tej kolejności: (A) render w procesie z przypiętej wersji GPUI (headless / `capture_screenshot`, jeśli istnieje), (B) prywatny Xvfb + `import -window root`; gdy sterownik sprzętowy nie prezentuje na Xvfb, programowy Vulkan (`vulkan-swrast`, wymuszony przez `VK_ICD_FILENAMES`), (C) realny ekran: tylko stwierdzenie blokady, nie rozwiązanie. Wynik (działająca ścieżka, komendy, czasy) zapisany w notatkach epiku.
- Spike testów: `#[gpui::test]` / `TestAppContext` przechodzi w `cargo test --manifest-path src-gpui/Cargo.toml -p media_tagger_gpui` bez ekranu.
- Skrypty `package.json`: `gpui:dev`, `gpui:build:e2e`, `gpui:fmt`, `gpui:clippy`, `gpui:check`, `test:client`, `test:gpui`, `audit:gpui`, `gpui:lock-parity`; `quality` i `test:all` rozszerzone o część GPUI. Skrypty `rust:*` i `test:backend` bez zmian.
- `src-tauri/tests/architecture_boundaries.rs` skanuje także `../src-gpui/crates/*`: manifest klienta bez crate'ów GUI (lista `GUI_CRATES` + `gpui*`, `wgpu*`, `blade*`); klient importuje z rdzenia tylko `models` i `error` poza `backend/core_backend.rs`; crate GPUI nie dotyka `db::`, blokad ani `usecases` poza kompozycją startu.
- ADR 0036 (drugi adapter GPUI w osobnym workspace, crate klienta, profile `.gpui.*`, GPUI z repozytorium Zed) z wpisem w `docs/adr/README.md`; `docs/architecture/system-overview.md`, `docs/development/setup-and-build.md`, `docs/development/testing.md`, `CLAUDE.md` (layout, komendy, profile); globy `src-gpui/**` w `.claude/rules/gpui-port.md`; `.gitignore` (`/src-gpui/target/`, `/src-gpui/target-e2e/`) i `scripts/privacy-check.py` (zakaz śledzenia `src-gpui/target*`).

## Poza zakresem

- Jakikolwiek ekran poza „hello”.
- Harness porównawczy (E01).
- Pakowanie aplikacji GPUI (E22).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src-tauri/Cargo.toml`
- `src-tauri/crates/media_tagger_core/Cargo.toml`
- `src-tauri/src/lib.rs`
- `src-tauri/src/app/profile.rs`
- `src-tauri/crates/media_tagger_core/src/app/bootstrap.rs`
- `src-tauri/tests/architecture_boundaries.rs`
- `e2e/build-e2e.js`
- `e2e/display.js`
- `packaging/flatpak/generate.py`
- `scripts/privacy-check.py`

Testy do przeniesienia:
- brak

Dokumenty i reguły:
- `docs/architecture/system-overview.md`
- `docs/adr/0034-tauri-independent-backend-core.md`
- `docs/adr/README.md`
- `docs/development/setup-and-build.md`
- `docs/development/testing.md`

## Kryteria akceptacji

- `git diff --stat` względem bazy nie pokazuje zmian w `src-tauri/Cargo.toml` ani `src-tauri/Cargo.lock`; `bun run test:packaging` zielone.
- `bun run gpui:dev` otwiera okno „Tagrove GPUI Dev” na profilu `com.example.mediatagger.gpui.dev`; `bun run tauri:dev` działa równolegle.
- `cargo tree --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client -e normal` nie zawiera gpui, wgpu, blade, tauri, gtk, webkit ani libmpv.
- `bun run gpui:lock-parity` zielone; jego test pokazuje, że pada po rozjechaniu wersji pakietu rdzenia.
- Notatki epiku zawierają wynik kontroli pochodzenia: przypięty `rev` i tag, lista źródeł git, lista nowych pakietów z licencjami, wynik `cargo audit`.
- Build debug z identyfikatorem spoza allowlisty `.gpui.dev`/`.gpui.e2e` odmawia startu (test).
- Opisana, powtarzalna ścieżka zrzutu ekranu okna GPUI bez realnego ekranu, albo epik `blocked` z dokładnym prerekwizytem (np. pakiet systemowy).
- `architecture_boundaries` pilnuje obu nowych crate'ów; snapshot wire bez zmian.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- `gpui` i towarzyszące mu crate'y z `https://github.com/zed-industries/zed`, wszystkie z jednego przypiętego `rev`

## Testy i sprawdzenia

Uruchamiane przez `job.py start E00 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`
- `bun run test:packaging`

## Scenariusze porównania

- brak (epik bez widocznego odpowiednika w Tauri albo bez UI)

## Ryzyka

- Zależność git: pierwszy `cargo fetch` klonuje całe repozytorium Zed (duże i wolne).
- RADV może nie prezentować na Xvfb (brak DRI3); `vulkan-swrast` nie jest zainstalowany i wymaga `sudo` użytkownika.
- `cargo audit` może zgłosić advisories z drzewa GPUI: nowe wyjątki tylko za zgodą użytkownika.
- Zed przypina toolchain 1.98.1, repozytorium 1.98.0.
- Dwa locki mogą się rozjechać w zależnościach rdzenia: stąd `gpui:lock-parity` w sprawdzeniach każdego epiku.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
