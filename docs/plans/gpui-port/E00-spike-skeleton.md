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

done 2026-11-20

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-20 — implementer

- Zrobione: workspace `src-gpui/` (`media_tagger_client`, `media_tagger_gpui`, własny `Cargo.lock`), okno „hello” na profilach `.gpui.dev`/`.gpui.e2e`, kompozycja startu (`boot.rs`), strażnik profilu, skrypty `gpui:*`/`test:client`/`test:gpui`/`audit:gpui`/`test:gpui-tools`, `scripts/gpui-lock-parity.py` z testem, `scripts/gpui-build-e2e.mjs` z testem, reguły GPUI w `architecture_boundaries`, ADR 0036, dokumentacja, `.gitignore`, `privacy-check.py`. `src-tauri/Cargo.toml` i `src-tauri/Cargo.lock` bez zmian.
- Odkryte (pierwszy krok): `*.workspace = true` w manifeście rdzenia rozwiązuje się z `src-tauri/Cargo.toml`, gdy rdzeń jest zależnością ścieżkową `src-gpui` (Cargo szuka korzenia workspace od katalogu pakietu). Bez zmian w manifeście rdzenia.
- Odkryte (toolchain): przypięty commit Zed deklaruje toolchain 1.98.1, ale buduje się i przechodzi testy na 1.98.0 z `rust-toolchain.toml`.
- Kontrola pochodzenia:
  - Pin: tag stabilny `v1.29.3` repozytorium `zed-industries/zed` → commit `d01b134cddf5e4f71964deb2d6f81cdb38fcf1e7` (tag lekki, wskazuje wprost na commit; sprawdzone `git ls-remote --tags`). Z Zed brane są `gpui` i `gpui_platform` (cechy `wayland`, `x11`, `font-kit`), reszta crate'ów Zed przechodnio z tego samego `rev`.
  - Źródła git w `src-gpui/Cargo.lock`: `zed-industries/zed` @ `d01b134c` (21 pakietów, wszystkie Apache-2.0: bench_metrics, collections, derive_refineable, gpui, gpui_apple, gpui_ios, gpui_linux, gpui_macos, gpui_macros, gpui_platform, gpui_shared_string, gpui_util, gpui_web, gpui_wgpu, gpui_windows, http_client, refineable, scheduler, sum_tree, ztracing, ztracing_macro) oraz repozytoria, które ten commit Zed sam przypina: `gfx-rs/wgpu` @ `cddb2f5d` (13 pakietów; upstream wgpu, nie fork Zed), `zed-industries/font-kit` @ `94b0f281`, `zed-industries/oo7` @ `731c1215`, `zed-industries/scap` @ `b5c86ec5`, `zed-industries/xim-rs` @ `16f35a2c` (3 pakiety). Listy pilnuje `PINNED_BY_ZED` w `scripts/gpui-lock-parity.py`.
  - `[patch.crates-io]` Zed: żaden wpis nie został powtórzony, bo build i testy przechodzą bez nich. Dotyczące naszego drzewa: `async-task`, `async-process`, `calloop`, `khronos-egl` (u nas wersje z crates.io: 4.7.1, 2.5.0, 0.14.5, 6.0.0) oraz `windows-capture` (tylko Windows). Fork `khronos-egl` usuwa panikę przy nieudanym wywołaniu EGL zgłaszającym `EGL_SUCCESS`; do rozważenia, jeśli ścieżka EGL (Xvfb) zacznie panikować.
  - Lock powstał z locka Zed dla tego commita (wersje testowane przez Zed) i z domknięcia rdzenia z `src-tauri/Cargo.lock`. Odstępstwa od locka Zed poza pakietami wspólnymi z lockiem Tauri: rodzina `wasm-bindgen` (`wasm-bindgen-futures`, `web-sys`, `js-sys`) w wersjach zgodnych z domknięciem rdzenia; `wayland-scanner` 0.31.11, `xcb` 1.7.1, `zbus`/`zbus_macros` 5.19.0, `zbus_names` 4.3.5, `zbus_xml` 5.2.1, `zvariant`/`zvariant_derive` 5.15.1, `zvariant_utils` 4.2.0, `zcheapstr` 1.3.0 (usuwają `quick-xml` < 0.41: RUSTSEC-2026-0194 i RUSTSEC-2026-0195); `spin` 0.9.9 i 0.10.1, `chacha20` 0.10.2 (wersje z locka Zed są wycofane z crates.io); `calloop` 0.14.5 i `windows-capture` 1.5.0 z crates.io zamiast forków z `[patch]`.
  - `cargo audit --file src-gpui/Cargo.lock` (baza RustSec z 2026-11-20, 697 pakietów): 0 podatności, bez żadnego `--ignore`. Ostrzeżenia informacyjne (nie blokują): unmaintained `chrono` 0.4.42, `paste` 1.0.15, `rustybuzz` 0.20.1, `ttf-parser` 0.25.1, `zbus_names` 4.3.5, `zvariant` 5.15.1, `zvariant_derive` 5.15.1; unsound `heapless` 0.9.2 (RUSTSEC-2026-0574…0582), `rustix` 1.1.4 (RUSTSEC-2026-0486, jest też w locku Tauri). Przed aktualizacją powyższych pakietów audit zgłaszał 6 podatności (obie advisories w `quick-xml` 0.30.0, 0.37.5, 0.39.3).
  - Pierwszy build nowych zależności: po `cargo fetch` w piaskownicy `bwrap --unshare-all` (bez sieci; katalog domowy jako tmpfs z repozytorium tylko do odczytu, zapis tylko do `src-gpui/target`, `~/.cargo` i `~/.rustup` tylko do odczytu), `cargo build --manifest-path src-gpui/Cargo.toml --locked --offline`: 453 crate'y, 20 s.
  - Pakiety, których nie ma w `src-tauri/Cargo.lock` w tej wersji (382, w tym 278 o nazwach nieobecnych w locku Tauri), według licencji z `cargo metadata`. Nie ma licencji copyleft innych niż MPL-2.0 (`cbindgen`: narzędzie budowania; `dwrote`: tylko Windows); `self_cell` jest dwulicencyjny (Apache-2.0 OR GPL-2.0-only).
  - MIT OR Apache-2.0 (188): accesskit 0.24.1, accesskit_atspi_common 0.19.1, accesskit_consumer 0.38.0, accesskit_macos 0.26.3, accesskit_unix 0.22.1, accesskit_windows 0.34.0, aes 0.8.4, ahash 0.8.12, allocator-api2 0.2.21, android_system_properties 0.1.5, anes 0.1.6, anstyle 1.0.13, as-raw-xcb-connection 1.0.1, ash 0.38.0+1.3.281, async-trait 0.1.89, backtrace 0.3.76, block-padding 0.3.3, borsh 1.8.1, c-enum 0.2.3, cast 0.3.0, cbc 0.1.2, chacha20 0.10.2, chrono 0.4.42, cipher 0.4.4, clap 4.5.49, clap_builder 4.5.49, clap_lex 0.7.6, const-random 0.1.18, const-random-macro 0.1.16, core-foundation 0.10.0, core-graphics 0.24.0, core-text 21.0.0, core_detect 1.0.0, cosmic-text 0.19.0, cpufeatures 0.3.0, crossbeam-queue 0.3.12, data-url 0.3.2, deranged 0.5.4, displaydoc 0.2.5, document-features 0.2.12, downcast-rs 1.2.1, enumn 0.1.14, etagere 0.2.15, euclid 0.22.14, fixedbitset 0.5.7, float-ord 0.3.2, font-types 0.10.0, font-types 0.11.0, foreign-types-macros 0.2.3, futures 0.3.34, futures-concurrency 7.7.1, gimli 0.32.3, gpu-allocator 0.28.0, hash32 0.3.1, hashbrown 0.14.5, hashbrown 0.16.1, heapless 0.9.2, hermit-abi 0.5.2, hkdf 0.12.4, hmac 0.12.1, http 1.3.1, iana-time-zone 0.1.64, inout 0.1.4, inventory 0.3.21, itertools 0.13.0, jni-sys 0.3.0, khronos-egl 6.0.0, lazy_static 1.5.0, litrs 1.0.0, lyon 1.0.16, lyon_algorithms 1.0.16, lyon_geom 1.0.17, lyon_path 1.0.16, lyon_tessellation 1.0.16, mac-notification-sys 0.6.15, md-5 0.10.6, memmap2 0.9.11, naga 30.0.0 (git), naga-types 30.0.0 (git), notify-rust 4.18.0, num 0.4.3, num-bigint-dig 0.9.1, num-conv 0.2.0, num-iter 0.1.45, num_cpus 1.17.0, page_size 0.6.0, pastey 0.2.3, pathdiff 0.2.3, pathfinder_geometry 0.5.1, pathfinder_simd 0.5.6, pbkdf2 0.12.2, perf-event-data 0.1.8, perf-event-open-sys2 5.0.6, perf-event2 0.7.4, piper 0.2.4, polycool 0.4.0, presser 0.3.1, proc-macro-crate 3.4.0, proptest 1.12.0, proptest-macro 0.6.0, quick-error 1.2.3, rand 0.10.2, rand_core 0.10.1, rand_xorshift 0.5.0, range-alloc 0.1.4, rangemap 1.7.1, raw-window-metal 1.1.0, read-fonts 0.35.0, read-fonts 0.37.0, ref-cast 1.0.25, ref-cast-impl 1.0.25, regex 1.12.3, renderdoc-sys 1.1.0, roxmltree 0.20.0, roxmltree 0.21.1, rustc-demangle 0.1.26, rusty-fork 0.3.1, scoped-tls 1.0.1, serde_bytes 0.11.19, serde_repr 0.1.20, serde_spanned 1.0.3, serde_urlencoded 0.7.1, siphasher 1.0.1, skrifa 0.37.0, skrifa 0.40.0, smol_str 0.3.6, static_assertions 1.1.0, svg_fmt 0.4.5, sys-locale 0.3.2, tauri-winrt-notification 0.7.3, time 0.3.47, time-core 0.1.8, toml 0.8.23, toml 0.9.8, toml_datetime 0.6.11, toml_datetime 0.7.3, toml_edit 0.22.27, toml_edit 0.23.7, toml_parser 1.0.4, toml_write 0.1.2, toml_writer 1.0.4, ttf-parser 0.25.1, unarray 0.1.4, unicode-bidi 0.3.18, unicode-bidi-mirroring 0.4.0, unicode-ccc 0.4.0, unicode-properties 0.1.3, unicode-script 0.5.8, unicode-vo 0.1.0, unicode-width 0.2.2, url 2.5.7, wait-timeout 0.2.1, web-time 1.1.0, wgpu 30.0.0 (git), wgpu-core 30.0.0 (git), wgpu-core-deps-apple 30.0.0 (git), wgpu-core-deps-emscripten 30.0.0 (git), wgpu-core-deps-linux-android-bsd 30.0.0 (git), wgpu-core-deps-wasm 30.0.0 (git), wgpu-core-deps-windows 30.0.0 (git), wgpu-hal 30.0.0 (git), wgpu-naga-bridge 30.0.0 (git), wgpu-sync 30.0.0 (git), wgpu-types 30.0.0 (git), windows 0.57.0, windows 0.62.2, windows-collections 0.3.2, windows-core 0.57.0, windows-future 0.3.2, windows-implement 0.57.0, windows-interface 0.57.0, windows-numerics 0.3.1, windows-registry 0.6.1, windows-result 0.1.2, windows-sys 0.48.0, windows-targets 0.48.5, windows-threading 0.2.1, windows_aarch64_gnullvm 0.48.5, windows_aarch64_msvc 0.48.5, windows_i686_gnu 0.48.5, windows_i686_msvc 0.48.5, windows_x86_64_gnu 0.48.5, windows_x86_64_gnullvm 0.48.5, windows_x86_64_msvc 0.48.5, wio 0.2.2, x11rb 0.13.2, x11rb-protocol 0.13.2, zed-font-kit 0.14.1-zed (git)
  - MIT (80): alloca 0.4.0, ashpd 0.13.5, block2 0.5.1, calloop 0.14.5, calloop-wayland-source 0.4.1, convert_case 0.10.0, convert_case 0.11.0, core_maths 0.1.1, dlib 0.5.3, embed-resource 3.0.6, endi 1.1.0, filedescriptor 0.8.3, float-cmp 0.9.0, float_next_after 1.0.0, fontconfig-parser 0.5.8, fontdb 0.23.0, freetype-sys 0.20.1, harfrust 0.5.0, http-body 1.0.1, imagesize 0.14.0, libredox 0.1.10, objc-sys 0.3.5, objc2 0.5.2, objc2 0.6.3, objc2-app-kit 0.2.2, objc2-core-data 0.2.2, objc2-core-image 0.2.2, objc2-foundation 0.2.2, objc2-metal 0.2.2, objc2-quartz-core 0.2.2, oo7 0.6.0 (git), oorandom 11.1.5, open 5.3.2, ordered-float 5.5.0, pico-args 0.5.0, plotters 0.3.7, plotters-backend 0.3.7, plotters-svg 0.3.7, postage 0.5.0, rustybuzz 0.20.1, schemars 1.0.4, schemars_derive 1.0.4, seahash 4.1.0, spin 0.10.1, spin 0.9.9, strict-num 0.1.1, strum 0.28.0, strum_macros 0.28.0, sysinfo 0.31.4, taffy 0.15.0, uds_windows 1.1.0, wayland-backend 0.3.15, wayland-client 0.31.11, wayland-cursor 0.31.11, wayland-protocols 0.32.9, wayland-protocols-plasma 0.3.9, wayland-protocols-wlr 0.3.9, wayland-scanner 0.31.11, wayland-sys 0.31.11, which 8.0.5, windows-capture 1.5.0, winnow 0.7.13, winnow 1.0.2, xcb 1.7.1, xcursor 0.3.10, xim-ctext 0.3.0 (git), xim-parser 0.2.1 (git), xkbcommon 0.8.0, xml-rs 0.8.28, xmlwriter 0.1.0, yeslogic-fontconfig-sys 6.0.0, zbus-lockstep 0.5.2, zbus-lockstep-macros 0.5.2, zbus_names 4.3.5, zbus_xml 5.2.1, zcheapstr 1.3.0, zed-scap 0.0.8-zed (git), zed-xim 0.4.0-zed (git), zvariant 5.15.1, zvariant_derive 5.15.1
  - Apache-2.0 OR MIT (47): addr2line 0.25.1, async-executor 1.13.3, async-fs 2.2.0, async-net 2.0.0, async-signal 0.2.13, atomic 0.5.3, atspi 0.29.0, atspi-common 0.13.0, atspi-proxies 0.13.0, bit-set 0.11.1, bit-vec 0.10.1, blocking 1.6.2, console_error_panic_hook 0.1.7, criterion 0.8.2, criterion-plot 0.8.2, ctor 1.0.12, fastrand 2.3.0, flume 0.12.0, idna_adapter 1.2.1, kurbo 0.13.1, linebender_resource_handle 0.1.1, link-section 0.19.2, linktime-proc-macro 0.2.2, ntapi 0.4.1, object 0.37.3, pin-project 1.1.10, pin-project-internal 1.1.10, pollster 0.2.5, pollster 0.4.0, portable-atomic 1.11.1, portable-atomic-util 0.2.4, resvg 0.46.0, rustc-hash 1.1.0, rustc-hash 2.1.1, signal-hook-registry 1.4.6, simplecss 0.2.2, smol 2.0.2, svgtypes 0.16.1, swash 0.2.6, tinytemplate 1.2.1, usvg 0.46.0, uuid 1.18.1, waker-fn 1.2.0, yazi 0.2.1, zeno 0.3.3, zeroize 1.8.2, zeroize_derive 1.4.2
  - Apache-2.0 (32): bench_metrics 0.1.0 (git), borsh-derive 1.8.1, ciborium 0.2.2, ciborium-io 0.2.2, ciborium-ll 0.2.2, codespan-reporting 0.13.0, collections 0.1.0 (git), derive_refineable 0.1.0 (git), gethostname 1.1.0, gl_generator 0.14.0, glutin_wgl_sys 0.6.1, gpui 0.2.2 (git), gpui_apple 0.1.0 (git), gpui_ios 0.1.0 (git), gpui_linux 0.1.0 (git), gpui_macos 0.1.0 (git), gpui_macros 0.1.0 (git), gpui_platform 0.1.0 (git), gpui_shared_string 0.1.0 (git), gpui_util 0.1.0 (git), gpui_web 0.1.0 (git), gpui_wgpu 0.1.0 (git), gpui_windows 0.1.0 (git), http_client 0.1.0 (git), khronos_api 3.1.0, refineable 0.1.0 (git), scheduler 0.1.0 (git), spirv 0.4.0+sdk-1.4.341.0, sum_tree 0.1.0 (git), unicode-linebreak 0.1.5, ztracing 0.1.0 (git), ztracing_macro 0.1.0 (git)
  - Unicode-3.0 (10): icu_locale_fallback 2.3.0, icu_locale_fallback_data 2.3.0, icu_provider 2.3.0, icu_segmenter 2.3.0, icu_segmenter_data 2.3.0, litemap 0.8.0, potential_utf 0.1.3, zerofrom 0.1.6, zerofrom-derive 0.1.6, zerovec-derive 0.11.5
  - Zlib OR Apache-2.0 OR MIT (6): bytemuck_derive 1.10.2, objc2-core-video 0.3.2, objc2-metal 0.3.2, objc2-security 0.3.2, objc2-uniform-type-identifiers 0.3.2, tinyvec 1.10.0
  - BSD-3-Clause (4): sha1_smol 1.0.1, subtle 2.6.1, tiny-skia 0.11.4, tiny-skia-path 0.11.4
  - Unlicense OR MIT (2): aho-corasick 1.1.3, termcolor 1.4.1
  - MPL-2.0 (2): cbindgen 0.28.0, dwrote 0.11.5
  - MIT OR Apache-2.0 OR Zlib (2): glow 0.18.0, xkeysym 0.2.1
  - Apache-2.0 OR MIT OR Zlib (2): macro_rules_attribute 0.2.3, macro_rules_attribute-proc_macro 0.2.3
  - BSD-2-Clause (1): arrayref 0.3.9
  - (Apache-2.0 OR MIT) AND BSD-3-Clause (1): encoding_rs 0.8.35
  - ISC (1): libloading 0.8.9
  - BSD-2-Clause OR MIT OR Apache-2.0 (1): mach2 0.6.0
  - Apache-2.0 OR GPL-2.0-only (1): self_cell 1.2.2
  - Zlib (1): slotmap 1.0.7
  - CC0-1.0 (1): tiny-keccak 2.0.2
- Spike zrzutów bez ekranu (oba warianty działają bez `vulkan-swrast`; prerekwizyt z README nie jest potrzebny):
  - (A) render w procesie: `gpui::HeadlessAppContext::with_platform(text_system, assets, || gpui_platform::current_headless_renderer())` + `capture_screenshot`; wymaga cechy `test-support` (`gpui`, `gpui_platform`), więc działa w celach testowych, nie w binarce. Komenda: `MEDIATAGGER_GPUI_SCREENSHOT_DIR=<katalog> MEDIATAGGER_GPUI_REQUIRE_RENDERER=1 cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test hello_window hello_window_renders_through_the_headless_renderer`. Czas testu 0,4–0,65 s; zrzut 2880×1800 (skala 2 platformy testowej). Działa na RADV bez ekranu, a w piaskownicy bez `/dev/dri` przez EGL z programowym rasteryzerem Mesy.
  - (B) prywatny Xvfb + `import -window root`: prawdziwa binarka na `dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1100x24 -nolisten tcp'` z tymczasowymi katalogami XDG i bez `WAYLAND_DISPLAY`. RADV nie prezentuje na Xvfb (brak DRI3), GPUI samo przechodzi na EGL (llvmpipe) i okno jest narysowane poprawnie (tło `#141618`, tekst „Loading...”). Okno widoczne 0,2–0,5 s po starcie, `import` 45–55 ms. Tytuł okna = tytuł profilu, `WM_CLASS` = identyfikator, geometria 1440×900; `WM_DELETE_WINDOW` zamyka okno i proces kończy się kodem 0, w katalogu profilu zostają `media.db`, `thumbs/`, `instance.lock`. Opis w `docs/development/testing.md` („Screenshots of the GPUI window without a real display”).
  - (C) realny ekran: nieużywany i niepotrzebny.
- Odkryte (GPUI): `gpui_platform::application()` bez serwera wyświetlania startuje w trybie headless i nigdy się nie kończy; adapter przed otwarciem profilu sprawdza `WAYLAND_DISPLAY`/`DISPLAY` (`require_display` w `src-gpui/crates/media_tagger_gpui/src/window.rs`) i uruchamia `gpui_platform::linux` tylko w trybie serwera, który odpowiedział. Po poprawkach z przeglądu (runda 1) sprawdzenie jest próbą połączenia z gniazdem: GPUI przy ustawionym `WAYLAND_DISPLAY` nie próbuje X11 i panikuje, gdy kompozytor nie odpowiada, więc martwy `WAYLAND_DISPLAY` obok żywego `DISPLAY` daje start na X11, a brak odpowiedzi kończy się błędem z kodem 1 przed utworzeniem danych profilu. Platforma testowa nie zapisuje tytułu z `TitlebarOptions`, więc tytuł jest testowany na `main_window_options`, a na żywym oknie przez Xvfb. `Application::run` na Linuksie wraca po zamknięciu ostatniego okna, więc `Boot` jest zwalniany po pętli zdarzeń; zwolnienie zamyka pulę bazy i oddaje blokadę instancji, ale nie zatrzymuje wątków miniatur (`ThumbnailScheduler` nie ma zamknięcia, wątki kończą się z procesem).
- Odkryte (parytet locków): poza wersjami różnią się cechy Cargo włączane w obu workspace'ach; pakiety obecne tylko w jednym domknięciu rdzenia (GPUI: `bytemuck_derive`, `futures-channel`, `rand_core` 0.10, `subtle`, `windows-sys` 0.48; Tauri: zależności deweloperskie `tempfile`) skrypt wypisuje jako notatki i nie uznaje za błąd.
- Decyzje: identyfikator jest wkompilowany (`build.rs`, `MEDIATAGGER_GPUI_IDENTIFIER`, domyślnie dev) i odrzucany poza allowlistą w każdym buildzie, nie tylko debug (brak identyfikatora wydania do E22); `media_tagger_gpui --identifier` wypisuje go bez startu; `serde_json` (klient) i `tempfile` (testy adaptera) to zależności już obecne w obu lockach w tych samych wersjach; `backend/core_backend.rs` i `boot.rs` są nazwami plików zapisanymi w regułach `architecture_boundaries`.
- Zostało: uruchomienie obok `bun run tauri:dev` na jednym pulpicie nie było wykonane (różne identyfikatory, katalogi i locki; test `profiles_in_different_directories_run_side_by_side` pokrywa tylko warstwę startu); sesja Wayland nie była sprawdzana (tylko X11 na Xvfb); `bun run test:all` uruchamia krok Gate.
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client` → pass (9); `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui` → pass (13 + 3); `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries` → pass (32); `bun run gpui:fmt`, `bun run gpui:clippy`, `bun run gpui:check`, `bun run gpui:lock-parity`, `bun run test:gpui-tools`, `bun run audit:gpui`, `bun run rust:fmt`, `bun run rust:clippy`, `bun run lint`, `bun run format:check`, `bun run locale:check`, `bun run test:packaging`, `bun run test:privacy` → pass; `bun run gpui:build:e2e` → pass (23 s), binarka zgłasza `com.example.mediatagger.gpui.e2e`; build z `MEDIATAGGER_GPUI_IDENTIFIER=com.example.mediatagger` kończy się kodem 1 („Refusing to start with identifier …”) i nie tworzy katalogu danych; `bun run gpui:dev` na prywatnym Xvfb → okno „Tagrove GPUI Dev”, kod wyjścia 0 po zamknięciu.

### 2026-11-20 — fixer (przegląd, runda 2)

- Zrobione: nieudane otwarcie głównego okna kończy pętlę zdarzeń i `run` zwraca błąd `cannot open the main window: …` (kod wyjścia 1, `Boot` zwolniony, blokada instancji oddana). Pętla jest wydzielona do prywatnego `run_application(application, open)` w `src-gpui/crates/media_tagger_gpui/src/window.rs`.
- Odkryte (GPUI): `cx.quit()` wywołane w domknięciu `Application::run` ginie. Na Linuksie `Platform::run` woła domknięcie przed `event_loop.run`, `quit` tylko ustawia flagę stopu calloop, a `calloop::EventLoop::run` (0.14.5) zeruje ją na starcie; bez okna `QuitMode::LastWindowClosed` też nie zadziała. Wyjście przed pierwszym obiegiem pętli trzeba zlecić z zadania: `cx.spawn(async |cx| cx.update(|cx| cx.quit())).detach()`, tak jak w testach platformy w Zed.
- Odkryte (testy): `gpui_platform::linux(WindowingModes::HEADLESS)` uruchamia prawdziwą pętlę zdarzeń bez serwera wyświetlania; tworzenie platformy zmienia środowisko procesu (`remove_var`), więc taki test biegnie w procesie potomnym (wzorzec `run_scenario` z `gpui_linux`).
- Zostało: nieudane `open_window` z prawdziwego renderera (brak adaptera wgpu) nie było wywołane na żywo; test wstrzykuje błąd otwarcia.
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui` → pass (23 + 3 + 8); nowy test `window::tests::a_window_that_cannot_open_ends_the_event_loop_with_its_error` przed poprawką → fail (pętla nie kończy się, proces potomny zatrzymany po limicie); `bun run gpui:fmt`, `bun run gpui:clippy` → pass.

### 2026-11-20 — workflow gpui-port-epic

- Zrobione: osobny workspace `src-gpui/` (własny `Cargo.lock`; `media_tagger_client` bez GUI i `media_tagger_gpui`), GPUI przypięte do Zed v1.29.3 (`d01b134cddf5e4f71964deb2d6f81cdb38fcf1e7`), okno powitalne z `common.loading` na ciemnym tokenie `--color-base-200`, start: strażnik identyfikatora, kontrola wyświetlacza, katalog danych, `InstanceLock`, `open_storage`, ffmpeg, workery. Skrypty `gpui-lock-parity` i `gpui-build-e2e`, ADR 0036, aktualizacje CLAUDE.md i dokumentacji. Dodano 12 testów (strażnik identyfikatora, kolejność startu, `boot.rs`, domknięcie zależności klienta, granice w `architecture_boundaries`); jedna zmiana produkcyjna: `run_with` w `media_tagger_gpui/src/lib.rs`. Przegląd: 3 rundy, 5 znalezisk potwierdzonych i naprawionych, brak otwartych.
- Odkryte: źródła git w locku to Zed oraz `PINNED_BY_ZED` (`gfx-rs/wgpu`, forki `font-kit`, `oo7`, `scap`, `xim-rs`); przy podbiciu Zed trzeba je wyprowadzić na nowo. Nie powtórzono wpisów `[patch.crates-io]` (np. fork `khronos-egl` – do rozważenia, gdy ścieżka EGL na Xvfb spanikuje). `audit:gpui` bez `--ignore` (RUSTSEC-2026-0194/0195 usunięte aktualizacją `wayland-scanner`, `xcb`, `zbus`/`zvariant`). 382 nowe pakiety względem locka Tauri, permisywne poza MPL-2.0 (`cbindgen`, `dwrote`). Zrzuty ekranu: w procesie przez `HeadlessAppContext` (tylko cele testowe, feature `test-support`, skala 2) albo prawdziwy plik binarny na prywatnym Xvfb (`import -window root`); E01 musi wybrać. Identyfikator wypalany przez `build.rs`, dozwolone tylko `.gpui.dev` i `.gpui.e2e`. Używać `gpui_platform::linux(WAYLAND | X11)` z kontrolą `WAYLAND_DISPLAY`/`DISPLAY`, bo `application()` bez wyświetlacza nie kończy się. `hello.rs`, `views/hello.rs`, `tests/hello_window.rs` to zaślepki dla E06; `i18n::Catalog` i `theme::css_color_token` to minimalne punkty startu dla E03. Pominięte przypadki testowe: zamknięcie okna z kodem 0 (potrzebna prawdziwa pętla zdarzeń), `gpui:dev` obok `tauri:dev`, testy skryptów już pokryte własnymi testami. Brak TypeScriptowych testów do przeniesienia.
- Zostało: nie uruchomiono równoległego `gpui:dev` i `tauri:dev` na jednym pulpicie ani sesji Wayland (tylko X11 na prywatnym Xvfb i renderer headless). Brak blokad, nieudanych testów i otwartych znalezisk przeglądu; brak raportu.
- Testy: `bun run quality` → pass; `bun run test:client` → pass (9); `bun run test:gpui` → pass; `cargo test -p media_tagger_client -p media_tagger_gpui --locked` → pass; `architecture_boundaries` → pass (33); `bun run test:packaging`, `bun run test:privacy` → pass; `bun run gpui:build:e2e` → pass; `bun run test:all` (bramka, 1 runda) → pass.
