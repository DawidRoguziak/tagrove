# Port interfejsu na GPUI: plan epików

Status: **w przygotowaniu** (2026-11-19). Plan, workflow i skill są gotowe; żaden epik nie został jeszcze uruchomiony. Bieżący stan jest w tabeli [Epiki](#epiki) (renderowanej z [epics.json](epics.json)) i w [NOTES.md](NOTES.md).

Odnośniki do kodu dotyczą commita `2b05641`. Przed edycją agent sprawdza je przez `rg`.

## Cel

1. Drugi interfejs Tagrove w GPUI (Rust, renderowanie na GPU, bez webview), który robi to samo co obecny interfejs Solid w Tauri.
2. Architektura heksagonalna zostaje: rdzeń `media_tagger_core` bez zmian, a GPUI jest kolejnym adapterem. Logika prezentacji trafia do crate'a bez zależności od GUI.
3. Aplikacja Tauri działa przez cały port bez zmian zachowania i kontraktu wire. Obie aplikacje da się uruchomić obok siebie i porównać na zrzutach ekranu.
4. Praca idzie epikami. Każdy epik uruchamia jeden workflow z tym samym przepływem: zmiana → testy → pętla testy/poprawki → pętla przeglądu → `test:all`.

## Werdykty z analizy

Analiza 2026-11-19: trzech agentów Explore (backend, frontend, narzędzia) i sprawdzenie stanu GPUI w crates.io.

- **Rdzeń jest gotowy na drugi adapter.** `media_tagger_core` nie ma Tauri, tokio ani async; wszystkie use case'y są synchroniczne i przyjmują `&AppState` albo `&Arc<DatabaseRuntime>`. Jedyne porty to `ProgressSink` (działa z domknięciem) i `ThumbHost`/`ThumbResponder`. Start bez Tauri to cztery wywołania: `InstanceLock::acquire`, `bootstrap::open_storage`, `resolve_ffmpeg_path`, `start_workers`.
- **Poza use case'ami adapter Tauri woła dwie funkcje serwisów:** `thumb_service::cancel_render_all_thumbnails` i `selection_service::apply_selection_mutation`.
- **Ślady webview w rdzeniu** (nieszkodliwe, zostają): nagłówki CORS w `ThumbReply`, bajtowy format `get_asset_query_id_range_service`, `owner_id`/`generation` w zapytaniach, klucze camelCase w DTO wejściowych, błędy spłaszczone do `String` z prefiksem `i18n:`.
- **Do odtworzenia w adapterze GPUI:** single-flight skanu startowego (`src-tauri/src/adapter/startup_scan.rs` stoi na `tauri::async_runtime`), strażnik profilu (`src-tauri/src/app/profile.rs` czyta konfigurację Tauri), cały stos wideo (`src-tauri/src/video/` jest związany z GTK i WebKit).
- **Frontend Solid to około 17 400 linii logiki i 19 500 linii testów.** Największe obszary: ustawienia (2 948), lightbox (2 812), galeria (2 478), bulk (1 740), wyszukiwarka (1 736), biblioteka (1 526). Testy są głównym nośnikiem specyfikacji zachowania.
- **43 komendy IPC, jedno zdarzenie** (`process-progress`), jeden kanał (wideo). W localStorage są tylko motyw i język.
- **Interfejs nie ma menu kontekstowych ani toastów.** Okno jest bezramkowe z własnymi kontrolkami. Kontrolki odtwarzania wideo rysuje strona natywna, nie Solid.
- **i18n:** 11 języków, 434 klucze, liczba mnoga CLDR (pl, ru, cs potrzebują `_few`/`_many`), interpolacja `{{x}}` bez escapowania.
- **Stan GPUI:** oficjalny crate `gpui` w crates.io (właściciel: zespół `zed-industries`) stoi na 0.2.2 z 2025-10-22, czyli jest o ponad rok za repozytorium. Aktualny kod jest tylko w repozytorium Zed. `gpui-pre` i `gpui-component` w crates.io publikuje jedna osoba trzecia (nie Zed); `gpui-pre` istnieje od 2026-09-03 i jest pobierany prawie wyłącznie jako zależność `gpui-component`.
- **Samo GPUI nie ma pola tekstowego**, ale w repozytorium Zed jest przykład `crates/gpui/examples/input.rs` (912 linii, Apache-2.0). Wirtualne listy (`uniform_list`, `list`) są wbudowane.
- **`packaging/flatpak/generate.py` odrzuca każde źródło spoza crates.io**, a `src-tauri/Cargo.lock` nie ma dziś żadnego źródła git. Zależność git we wspólnym locku zepsułaby pakowanie obecnej aplikacji.
- **Zrzuty ekranu:** `app:control` robi je przez X11 (`import -window root`), więc ta sama metoda złapie okno GPUI. Sterowanie przez WebDriver nie przenosi się. Na tej maszynie jest tylko sprzętowy Vulkan (RADV); programowy (`vulkan-swrast`) nie jest zainstalowany, a prezentacja RADV na Xvfb jest niesprawdzona.
- **Narzędzia przeglądu nie znają GPUI:** `review-frontend` jest pisany pod Solid, żadna reguła z `.claude/rules/` nie ładuje się dla nowych crate'ów, `collect_scope.py` nie proponuje soczewki GPUI.
- **`bun run test:all`** = `quality` + Vitest + `cargo test` + desktop E2E; potrzebuje prywatnego displaya i trwa dłużej niż limit 10 minut jednej komendy agenta.

## Stan docelowy

```
src-tauri/                              workspace Tauri: BEZ ZMIAN (Cargo.toml, Cargo.lock, target/)
  src/                                  ADAPTER TAURI
  crates/media_tagger_core/             RDZEŃ (zmiany tylko addytywne, gdy port ich wymaga)
  tests/architecture_boundaries.rs      + reguły dla klienta i adaptera GPUI (skanuje ../src-gpui)
src-gpui/                               NOWY, osobny workspace: własny Cargo.toml, Cargo.lock, target/, target-e2e/
  Cargo.toml                            [workspace]; gpui z repozytorium Zed przypięty przez rev;
                                        media_tagger_core jako zależność ścieżkowa ../src-tauri/crates/media_tagger_core
  crates/media_tagger_client/           KLIENT: logika prezentacji bez GUI
    backend/                            port Backend (trait), CoreBackend (jedyny plik sięgający do use case'ów), FakeBackend
    exec/                               wykonawca: produkcyjny i deterministyczny do testów
    i18n/, theme/, prefs/               teksty z src/i18n/locales, tokeny motywu, preferencje UI
    text/                               model edycji tekstu dla własnego pola tekstowego
    library/                            zapytania i cache stron, szczegóły, stan tagów, mutacje, słownik
    gallery/, search/, bulk/, lightbox/, settings/   modele funkcji jako maszyny stanów
    runtime.rs                          AppRuntime: modele na czas życia aplikacji
  crates/media_tagger_gpui/             ADAPTER GPUI: binarka media_tagger_gpui
    main.rs, boot.rs, profile.rs        kompozycja startu, strażnik profilu, katalog danych
    ui/                                 prymitywy, pole tekstowe, ikony, stos warstw, modal
    views/                              powłoka, galeria, wyszukiwarka, dok, lightbox, ustawienia
    thumbs.rs                           ładowanie i cache miniatur dla elementów obrazu
    scenario.rs                         deterministyczne stany startowe (tylko profil .gpui.e2e)
src/                                    frontend Solid: bez zmian; locale i styles.css są wspólnym źródłem
e2e/compare/                            harness porównawczy (E01)
```

Kierunek zależności: `media_tagger_gpui` → `media_tagger_client` → `media_tagger_core`. Klient nie zna GPUI; rdzeń nie zna klienta.

Dwa workspace'y to dwa pliki `Cargo.lock`. Rdzeń kompiluje się w obu, więc `bun run gpui:lock-parity` (E00) pilnuje, żeby wersje jego zależności były w obu lockach identyczne.

Nazwy podkatalogów w crate'ach są szkicem; ustala je epik, który je tworzy.

## Decyzje

Decyzje podjęte przy tworzeniu planu. Każdą można zmienić przed E00; po E00 zmiana wymaga ADR.

| # | Pytanie | Decyzja | Uzasadnienie |
| --- | --- | --- | --- |
| D1 | Skąd GPUI | `gpui` z repozytorium `zed-industries/zed` jako zależność git przypięta do commita tagu stabilnego wydania (`rev`); bez `gpui-pre` i bez `gpui-component`. Pole tekstowe piszemy sami (E04) | Pochodzenie kodu jest pewne: jedynym wydawcą jest Zed. Oficjalny crate w crates.io ma ponad rok; `gpui-pre`/`gpui-component` publikuje jedna osoba trzecia. Wybrane 2026-11-19 po pytaniu użytkownika o pochodzenie zależności. |
| D2 | Gdzie żyje kod | Osobny workspace `src-gpui/` z crate'ami `media_tagger_client` i `media_tagger_gpui`, własnym `Cargo.lock` i katalogami `target`; rdzeń jako zależność ścieżkowa. Workspace `src-tauri/` bez zmian | Lock, audyt i pakowanie wydawanej aplikacji Tauri nie widzą drzewa zależności portu ani źródła git; build Tauri i E2E nie płacą za GPUI. Granicy warstw pilnuje kompilator (jak w ADR 0034). Decyzja użytkownika z 2026-11-19. |
| D3 | Jak UI rozmawia z rdzeniem | Port sterujący `Backend` w kliencie; `CoreBackend` woła use case'y bezpośrednio | Brak IPC i serializacji; `FakeBackend` daje testy kolejności, prawdziwy rdzeń na tempfile daje testy integracyjne. |
| D4 | Model współbieżności | Modele klienta to maszyny stanów: polecenie → stan + efekty, odpowiedzi z tokenem żądania/generacji, timery jako efekty. Bez tokio | Odwzorowuje istniejące ID żądań i generacje z Solid; testy są deterministyczne; rdzeń zostaje synchroniczny. |
| D5 | Profile i dane | Nowe identyfikatory `com.example.mediatagger.gpui.dev` i `.gpui.e2e` (tytuły „Tagrove GPUI Dev” / „Tagrove GPUI E2E”); bez identyfikatora wydania do E22 | Obie aplikacje działają jednocześnie (osobne `instance.lock`); błąd w porcie nie dotknie prawdziwej biblioteki. |
| D6 | Preferencje UI | Plik w katalogu profilu przez port `PrefsStore`; tylko motyw i język | Parytet z localStorage (`media-tagger.theme`, `media-tagger.language`). |
| D7 | Teksty | Jedno źródło: `src/i18n/locales/*.json` wkompilowane przez `include_str!`; własny silnik zgodny z używanym podzbiorem i18next | `bun run locale:check` dalej pilnuje kluczy dla obu aplikacji. |
| D8 | GIF i wideo | Siatka: statyczna miniatura + chip (jak dziś). Lightbox: GIF jako pierwsza klatka z chipem, wideo jako panel-zaślepka z plakatem i metadanymi. Animacja i odtwarzanie odroczone (E20, E21) | Decyzja pozostawiona przez użytkownika; libmpv jest dziś związany z GTK/WebKit i wymaga osobnego projektu. Panel boczny działa dla wszystkich typów. |
| D9 | Tauri w trakcie portu | Bez zmian zachowania i wire; `test:all` zielone po każdym epiku | Tauri jest wzorcem do porównań i siatką bezpieczeństwa. |
| D10 | Porównanie wizualne | Harness `compare:ui` (E01) i katalog scenariuszy; agent ocenia pary PNG. Parytet układu, tokenów i treści, nie pikseli | Inny rasteryzator fontów wyklucza porównanie piksel w piksel. |
| D11 | Commity | Workflow nigdy nie commituje. Epik startuje na czystym drzewie, baza przeglądu = `HEAD`. Commit robi użytkownik albo `/gpui-port <epik> --commit` po kontroli prywatności | Reguły repozytorium; czysta baza trzyma zakres przeglądu w granicach epiku. |
| D12 | Równoległość | Jeden epik naraz, jeden agent piszący naraz; równolegle tylko agenci czytający (soczewki, weryfikatorzy) | Zakaz worktree, wspólne katalogi `target`, porty i locki desktop E2E. |
| D13 | Podpowiedzi tagów | Dopasowanie rozmyte w Rust w zadaniu tła zamiast workera Fuse.js | Liczą się oczekiwania z testów, nie identyczne punkty rankingu. |
| D14 | Okno | Dekoracje po stronie klienta z własnymi kontrolkami | Parytet z bezramkowym oknem Tauri. |
| D15 | Dialogi i linki | API GPUI (portal XDG) | Bez dodatkowej zależności; zmiana tylko, jeśli E00 wykaże brak. |

## Czego nie robimy

- Zmian zachowania aplikacji Tauri, kontraktu wire ani tekstów błędów.
- async, tokio, traitów repozytoriów ani nowych portów fs/zegara w rdzeniu (ADR 0034).
- Biblioteki komponentów od osób trzecich (`gpui-component`) i snapshotów `gpui-pre`.
- Zmian w `src-tauri/Cargo.toml` i `src-tauri/Cargo.lock` na potrzeby GPUI.
- Animacji GIF i odtwarzania wideo w GPUI przed E20/E21.
- Identyfikatora wydania, pakowania i usuwania Tauri przed E22.
- Porównań piksel w piksel i automatycznych progów różnic obrazu.
- Worktree i równoległych agentów piszących.
- Automatycznych commitów z workflow.

## Egzekucja granicy

Od E00 pilnują jej kompilator i `src-tauri/tests/architecture_boundaries.rs`:

- Test żyje w workspace Tauri i skanuje źródła oraz manifesty w `../src-gpui/crates/`.
- `media_tagger_client` nie ma w manifeście crate'ów GUI (obecna lista `GUI_CRATES` plus `gpui*`, `wgpu*`, `blade*`).
- W kliencie tylko `backend/core_backend.rs` importuje `media_tagger_core::{usecases, services, app}`; reszta używa `models`, `error` i własnego portu.
- `media_tagger_gpui` nie zawiera `db::`, helperów blokad ani wywołań use case'ów poza kompozycją startu.
- `media_tagger_core` nie zależy od żadnego z nowych crate'ów (kompilator).
- Snapshot wire Tauri (`REGISTERED_COMMANDS`, `COMMAND_SIGNATURES`, DTO) nie zmienia się w żadnym epiku.
- `src-tauri/Cargo.lock` nie dostaje żadnego pakietu z drzewa GPUI ani źródła git (`bun run test:packaging`).
- `bun run gpui:lock-parity`: zależności rdzenia w tych samych wersjach w obu lockach.

## Epiki

<!-- epics:start -->
| Epik | Nazwa | Rozmiar | Poziom | Zależy od | Status |
| --- | --- | --- | --- | --- | --- |
| E00 | [Spike stosu i osobny workspace GPUI](E00-spike-skeleton.md) | L | trudny | — | done 2026-11-20 |
| E01 | [Harness porównawczy Tauri ↔ GPUI](E01-compare-harness.md) | L | trudny | E00 | done 2026-11-20 |
| E02 | [Port Backend, wykonawca i potok miniatur (crate klienta)](E02-client-backend-port.md) | L | trudny | E00 | todo |
| E03 | [i18n, tokeny motywu i preferencje UI](E03-client-i18n-theme-prefs.md) | M | standardowy | E00 | todo |
| E04 | [Pole tekstowe (własny element GPUI)](E04-text-input.md) | M | trudny | E03 | todo |
| E05 | [Prymitywy UI i warstwa nakładek (GPUI)](E05-gpui-primitives-overlays.md) | L | standardowy | E03, E04 | todo |
| E06 | [Runtime aplikacji, okno i powłoka](E06-gpui-app-shell.md) | M | standardowy | E02, E05 | todo |
| E07 | [Model zapytań i cache stron](E07-client-query-model.md) | L | trudny | E02 | todo |
| E08 | [Szczegóły, stan tagów, mutacje i słownik](E08-client-metadata.md) | L | trudny | E07 | todo |
| E09 | [Wirtualna siatka galerii i miniatury](E09-gpui-gallery-grid.md) | L | trudny | E06, E07 | todo |
| E10 | [Pasek przewijania i scrubber osi czasu](E10-gpui-scrollbar-timeline.md) | M | standardowy | E09 | todo |
| E11 | [Model zaznaczenia i gesty](E11-selection.md) | L | trudny | E08, E09 | todo |
| E12 | [Wyszukiwarka, gramatyka filtrów i podpowiedzi](E12-search.md) | L | standardowy | E06, E08 | todo |
| E13 | [Modal listy tagów i wspólny TagEditor](E13-tag-list-editor.md) | M | standardowy | E12 | todo |
| E14 | [Dok akcji zbiorczych i kolejność grup](E14-bulk-dock.md) | L | trudny | E11, E13 | todo |
| E15 | [Model lightboxa](E15-client-lightbox-model.md) | L | trudny | E08 | todo |
| E16 | [Lightbox: UI, przeglądarka obrazów, zaślepki GIF i wideo](E16-gpui-lightbox.md) | L | trudny | E09, E13, E15 | todo |
| E17 | [Ustawienia: szkielet, skan, miniatury, wygląd, język, strefa zagrożeń, o programie](E17-settings-core.md) | L | standardowy | E02, E06 | todo |
| E18 | [Ustawienia: import/eksport, przywracanie i duplikaty](E18-settings-data.md) | L | trudny | E17 | todo |
| E19 | [Domknięcie: parytet, wydajność, klawiatura, dokumentacja](E19-acceptance.md) | L | trudny | E10, E14, E16, E18 | todo |
| E20 | [Animacja GIF w lightboxie](E20-gif-animation.md) | M | standardowy | E16 | deferred |
| E21 | [Natywne wideo (libmpv) w oknie GPUI](E21-native-video.md) | L | trudny | E16 | deferred |
| E22 | [Przełączenie: identyfikator wydania, pakowanie, usunięcie Tauri](E22-cutover.md) | L | trudny | E19, E21 | deferred |
<!-- epics:end -->

Statusy: `todo`, `in progress`, `blocked`, `done`, `deferred`. Zmienia je tylko `port_state.py set-status` (manifest, ta tabela, plik epiku i [NOTES.md](NOTES.md) naraz).

Kolejność: E00 → E01 → E02, E03 → E04 → E05 → E06 → E07 → E08 → E09 → dalej według zależności. Epiki modeli (E07, E08, E15) nie mają UI, więc można je robić, zanim powstanie odpowiadający im ekran. E20–E22 są odroczone i wymagają nowej decyzji.

## Przepływ epiku

Jeden przebieg workflow [`gpui-port-epic`](../../../.claude/workflows/gpui-port-epic.js) to jeden epik:

1. **Implement.** Jeden agent buduje zakres epiku. Specyfikacją jest kod Solid i jego testy.
2. **Tests.** Drugi agent, który nie pisał implementacji, przenosi przypadki z testów TS na najniższą warstwę testów Rust i dopisuje testy kryteriów akceptacji.
3. **Stabilize.** Pętla: `job.py checks` → poprawki, aż wszystko przejdzie (domyślnie do 5 rund). Epiki ze scenariuszami dostają potem porównanie zrzutów Tauri ↔ GPUI i do 2 rund poprawek różnic.
4. **Review.** Pętla do 6 rund, odwzorowanie skilla `/review-changes`: zakres z `collect_scope.py`, niezależne soczewki (`review-conventions`, `review-rust`, `review-gpui`, warunkowo wydajność, stabilność, narzędzia, frontend), jeden `review-verifier` na ustalenie, poprawki potwierdzonych, ponowne sprawdzenia. Runda bez potwierdzonych ustaleń kończy pętlę. Po ostatniej rundzie pozostałe P0–P2 trafiają do [reports/](reports/README.md).
5. **Gate.** `bun run test:all` na prywatnym displayu (`job.py test-all`) → poprawki (domyślnie do 3 rund).
6. **Record.** Notatki w pliku epiku, raport otwartych ustaleń, status.

Epik kończy się jako `done` tylko z zielonym `test:all` i bez otwartego P0. W każdym innym przypadku dostaje `blocked` z powodem w notatkach.

## Jak odpalać

```
/gpui-port status                 stan epików i następny gotowy do startu
/gpui-port E00                    cały przepływ dla epiku
/gpui-port next                   pierwszy epik ze spełnionymi zależnościami
/gpui-port E07 --from review      wznowienie od kroku (implement, tests, stabilize, review, gate)
/gpui-port E07 --commit           po sukcesie: kontrola prywatności i commit epiku
/gpui-port E07 --models implementer=fable:xhigh,lens=opus
/gpui-port E00 --dry-run          przebieg kontrolny przepływu bez zmian w repozytorium
```

Skill: [`.claude/skills/gpui-port/SKILL.md`](../../../.claude/skills/gpui-port/SKILL.md). Role i modele: [`references/models.json`](../../../.claude/skills/gpui-port/references/models.json). Stan roboczy przebiegu (logi zadań, baza przeglądu) leży w ignorowanym `docs/review/gpui-port/<epik>/`.

## Scenariusze porównania

Katalog jest w [epics.json](epics.json) (`scenarios`). Scenariusz to nazwany, deterministyczny stan obu aplikacji na tym samym syntetycznym seedzie. E01 buduje harness i pierwsze dwa scenariusze; każdy epik UI dopisuje kroki swoich scenariuszy po stronie Tauri (`e2e/compare/scenarios/`) i stan startowy po stronie GPUI.

Dodanie scenariusza (harness z E01, opis w [testing.md](../../development/testing.md#comparison-harness-tauri-and-gpui)):

1. wpis w `scenarios` w [epics.json](epics.json) (jeśli go jeszcze nie ma) i w `parity` epiku;
2. plik `e2e/compare/scenarios/<id>.js` z polami `id`, `theme`, `seed` (`empty` albo `control`) i `tauri` (komendy `app:control`, które doprowadzają aplikację Tauri do tego stanu po zastosowaniu motywu i seeda);
3. wpis w `SCENARIOS` w `src-gpui/crates/media_tagger_client/src/scenario.rs` ze stanem startowym GPUI (`media_tagger_gpui --scenario <id>`, tylko profil `.gpui.e2e`);
4. `bun run test:compare` i `bun run test:client` pilnują zgodności tych trzech miejsc; `bun run compare:ui <id>` robi zrzuty do `artifacts/gpui-compare/<id>/`.

Agent parytetu traktuje zrzut Tauri jako wzorzec i zgłasza różnice widoczne dla użytkownika: brakujące albo nadmiarowe elementy, kolejność, wyrównanie, token koloru, odstępy, tekst, stan. Pomija rasteryzację fontów, przesunięcia subpikselowe i zaślepki z D8.

## Prerekwizyty poza repozytorium

- Jeśli E00 wykaże, że sprzętowy Vulkan nie prezentuje na Xvfb: pakiet `vulkan-swrast` (instalacja przez użytkownika, wymaga `sudo`).
- Narzędzia już obecne: `Xvfb`, `xvfb-run`, `import` (ImageMagick), `dbus-run-session`, `ffmpeg`, `tauri-driver`, `WebKitWebDriver`.

## Zasady dla agentów

- Przed startem przeczytaj ten README, plik swojego epiku, [NOTES.md](NOTES.md) i notatki epików, od których zależysz.
- **Tauri zostaje nietknięte.** Nie zmieniaj zachowania, kontraktu wire ani testów w `src/`, `e2e/` i `src-tauri/src/`, chyba że plik epiku wymienia dany plik. Zmiany w rdzeniu tylko addytywne.
- **Warstwy:** klient bez GUI i bez sięgania do rdzenia poza portem; adapter GPUI bez `db::`, blokad i use case'ów poza startem. Nigdy nie blokuj wątku UI wywołaniem backendu.
- **Najpierw backend:** model publikuje stan dopiero po sukcesie backendu, z kanonicznego wyniku; spóźnione odpowiedzi odrzuca po tokenie.
- **API GPUI szybko się zmienia.** Przed użyciem API przeczytaj źródła i przykłady przypiętego commita Zed w checkoucie cargo (`~/.cargo/git/checkouts/zed-*/<rev>/crates/gpui/`), nie polegaj na pamięci.
- **Dwa workspace'y:** komendy cargo dla portu biorą `--manifest-path src-gpui/Cargo.toml`. `src-tauri/Cargo.toml` i `src-tauri/Cargo.lock` zmieniają się tylko wtedy, gdy plik epiku wymienia addytywną zmianę rdzenia.
- **Zależności:** `cargo` zawsze z `--locked`. Nowa zależność tylko z listy „Dozwolone nowe zależności” epiku; inaczej zatrzymaj się i zgłoś `blocked`.
- **Teksty** tylko z `src/i18n/locales` (11 plików, te same klucze i placeholdery).
- **Uruchomienia desktopowe są seryjne.** Prywatny display i tymczasowe katalogi XDG, nigdy realny ekran i realne dane. `bun run test:all` i pełne `test:e2e:tauri` uruchamia tylko krok Gate przez `job.py`. Nie zabijaj procesów po nazwie i nie usuwaj cudzych locków.
- **Testy niszczące** (usuwanie, czyszczenie, przywracanie, duplikaty) tylko na świeżych katalogach tymczasowych.
- **Repozytorium jest publiczne:** żadnych prywatnych ścieżek ani danych w kodzie, notatkach i fixture'ach.
- **Zakaz worktree. Zakaz commitów, stage, push, stash i reset** w agentach workflow.
- **Nazwy wewnętrzne** (`media_tagger*`, `com.example.mediatagger*`, `MEDIATAGGER_*`) zostają; nazwa widoczna dla użytkownika to Tagrove.
- Decyzje z kryteriów `docs/adr/README.md` zapisuj jako ADR w tej samej zmianie.

## Protokół notatek

Plik epiku kończy się sekcjami `## Status` i `## Notatki agentów`.

- `## Status` zmienia tylko `port_state.py set-status`.
- `## Notatki agentów`: wpisy tylko dopisywane:

  ```
  ### YYYY-MM-DD — <rola>
  - Zrobione: …
  - Odkryte: … (plik:linia)
  - Zostało: …
  - Testy: `<komenda>` → pass/fail
  ```

- Odkrycie, które zmienia inny epik, zapisz w notatkach tamtego epiku z prefiksem `Z epiku ENN:`.
