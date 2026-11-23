# E17: Ustawienia: szkielet, skan, miniatury, wygląd, język, strefa zagrożeń, o programie

Rozmiar: L. Poziom: standardowy. Zależy od: E02, E06.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Panel ustawień z jednym runnerem operacji i sekcjami, które nie niszczą danych poza „wyczyść bibliotekę”.

## Zakres

- Panel z nawigacją śledzącą przewijanie; runner operacji: jedna naraz, subskrypcja postępu przed wywołaniem, tylko własne fazy, sukces wyłącznie z wyniku komendy.
- Skan: dodaj folder (dialog systemowy), usuń root (potwierdzenie), auto-skan per root, skanuj ponownie, skanuj wszystko. Miniatury: renderuj wszystkie, ponów nieudane, zatrzymaj, wyczyść; status narzędzi wideo.
- Wygląd (jasny/ciemny), język (11), o programie (wersja, licencja, linki przez `open_url`).
- Strefa zagrożeń: wyczyść bibliotekę z potwierdzeniem, przez barierę mutacji tagów.

## Poza zakresem

- Import/eksport i duplikaty (E18).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/settings/SettingsPanel.tsx`
- `src/features/settings/SettingsNavigation.tsx`
- `src/features/settings/createSettings.ts`
- `src/features/settings/createSettingsOperationRunner.ts`
- `src/features/settings/progressService.ts`
- `src/features/settings/operationStateService.ts`
- `src/features/settings/statusMessage.ts`
- `src/features/settings/SectionOperationStatus.tsx`
- `src/features/settings/createScanSettings.ts`
- `src/features/settings/RemoveScanRootConfirmDialog.tsx`
- `src/features/settings/createDangerZoneSettings.ts`
- `src/features/settings/ClearLibraryConfirmDialog.tsx`
- `src/features/settings/sections`
- `src/app/AppSettingsView.tsx`
- `src/platform/tauri/dialogs.ts`

Testy do przeniesienia:
- `src/features/settings/__tests__`

Dokumenty i reguły:
- `docs/subsystems/settings-operations.md`
- `docs/subsystems/scanning-and-indexing.md`
- `.claude/rules/settings.md`
- `.claude/rules/scanning.md`

## Kryteria akceptacji

- Testy runnera, postępu i modeli skanu przeniesione.
- Druga operacja w trakcie pierwszej jest odrzucana synchronicznie.
- Testy „wyczyść bibliotekę” tylko na świeżych katalogach tymczasowych.
- Parytet `settings-scan`, `settings-appearance-language`, `settings-danger-about`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E17 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `settings-scan`: Ustawienia, sekcja skanu z jednym rootem i statusem operacji.
- `settings-appearance-language`: Sekcje wyglądu i języka.
- `settings-danger-about`: Strefa zagrożeń i „o programie”.

## Ryzyka

- Dialogi systemowe (portal XDG) nie działają na prywatnym displayu: w scenariuszach wstrzykiwać ścieżkę.

## Status

done 2026-11-23

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E05

- Z epiku E05: przełącznik „Scan on app startup” to `ui::switch::Switch` (`toggle toggle-sm toggle-primary`, kontrolowany przez `on_change`), status operacji: `ui::spinner::Spinner` z `SpinnerSize::ExtraSmall` i `ui::progress::ProgressBar`, dialogi potwierdzeń: `ui::modal`. Karty wyboru motywu (`.theme-choice`, `.theme-preview--*`) i `.theme-select` z `--select-chevron` nie powstały w E05. Strona ustawień rejestruje warstwę niemodalną (`ui::layers`), patrz notatka E05 w E06.

### 2026-11-23 — workflow gpui-port-epic

- Zrobione: model `Settings` w `AppRuntime` (klient `media_tagger_client::settings`) z wyłącznym runnerem operacji: druga operacja jest odrzucana w tej samej turze, postęp jest subskrybowany przed wywołaniem, sukces wynika tylko z wyniku komendy, a po operacji następuje odświeżenie biblioteki i/lub rootów. Operacje: dodanie folderów, przełącznik skanu przy starcie per root, rescan jednego/wszystkich rootów, usunięcie roota (z potwierdzeniem), render/ponowienie miniatur, Stop renderu i czyszczenie biblioteki za barierą mutacji tagów. Skan startowy jest teraz operacją runnera. Widok GPUI (`views/settings.rs`): nawigacja z zaznaczeniem sekcji, sekcje skanu, wyglądu (karty motywów), języka, strefy zagrożeń i „o programie”, modale potwierdzeń. Dodane scenariusze porównania `settings-scan`, `settings-appearance-language`, `settings-danger-about` (pliki Tauri, `SCENARIOS`, `ScenarioSettings`). Testy: 53 nowe testy Rust (40 klient, 13 adapter) pokrywające 68 przypadków z `src/features/settings/__tests__` oraz `settings_core.rs`, `settings_window.rs`, `settings_render.rs`. Dokumentacja: `settings-operations.md`, `scanning-and-indexing.md`, `testing.md`.
- Odkryte: (1) Model ustawień przyjmuje `RuntimeInput::Settings`; E18 dodaje import/eksport i duplikaty do tego samego runnera (rezerwowane `OperationSection::ImportExport` i `Duplicates`). (2) Bilety komend biblioteki ustawień zaczynają się od `SETTINGS_TICKET_BASE = 1 << 59`; import CSV i restore w E18 mają używać `BeginMaintenance`/`EndMaintenance` z `LibraryReset::Identity` i `Restored`. (3) `RuntimeEffect::ScanStartupRoots` niesie `progress: Option<OperationId>`; start kolejkuje 6 zadań (dodatkowo odczyt narzędzi wideo). (4) Dialog folderu należy do adaptera (`take_folder_picker_request`, `prompt_for_paths`); tytuł dialogu ustawia GPUI. (5) Układ: bezpośrednie dzieci przewijanego kontenera to dokładnie 7 sekcji w kolejności `SettingsSection::ALL`; E18 ma wypełnić puste kotwice sekcji 3 i 4 bez dodawania ani zmiany kolejności dzieci. (6) Odłożone: sekcje import/eksport i duplikaty (E18); brak fallbacku błędu otwarcia linku w About (`cx.open_url` nie zwraca błędu); klik w nawigację nie przenosi fokusu klawiatury; brak płynnego przewijania przy podświetleniu „Add first folder”; brak tooltipa ze ścieżką roota. (7) Pominięte przypadki testów: listener/dispose Solid, warianty CSV/restore bariery, animacja obecności modala, DOM/ARIA/URL hash, rozwijanie tekstów prawnych (powody w raporcie testów; tylko przypadki specyficzne dla Solid/webview lub należące do E18). (8) Różnice parity zostawione jako minor: pusta przerwa ok. 60 px w miejscu sekcji E18 (settings-appearance-language, settings-danger-about); karta Appearance ląduje ok. 19 px niżej po kliknięciu nawigacji; dolny odstęp ok. 30 px zamiast ok. 48 px; ikony i odstępy nawigacji oraz nagłówek o kilka px mniejsze/przesunięte (wszystkie scenariusze).
- Zostało: nic nie blokuje (status done, blocked: null). Brak otwartych, spornych, wcześniejszych ani niepewnych ustaleń review (5 potwierdzonych, 5 naprawionych w 4 rundach, review czyste), więc raport nie powstał. Gate zielony po 1 rundzie. Zob. punkty odłożone wyżej (E18 i drobne różnice parity).
- Testy: cargo fmt src-gpui — pass; cargo clippy src-gpui (-D warnings) — pass; 46 binariów testowych src-gpui — pass; architecture_boundaries (33/33) — pass; bun run test:compare (54/54) — pass; testy klienta i adaptera dodane w fazie testów — pass (53/53); gate test:all — pass (1 runda).

### 2026-11-23 — poprawki po audycie orkiestratora (E17)

- Zrobione:
  - G1 (nawigacja): przyczyna to kaskada Tailwinda, nie odstępy. `UiIcon` ma własne `h-5 w-5`, które leży w arkuszu za `h-4 w-4` podanym przez link, więc ikona w Tauri ma 20 px, a nie 16 px. Ikona wyznacza też wysokość linku (20 + 2 × 10 = 40 px; linia tekstu ma 19,5 px), stąd krok 44 px i koniec karty na y 417. `layout::nav::ICON_SIZE` = 20, nowe `LINK_HEIGHT`. Po zmianie karta nawigacji zajmuje y 96–417 w obu aplikacjach, etykiety zaczynają się na x 75.
  - G2 i G3 (cel przewijania, dół strony): jedna przyczyna, i nie jest nią ani przesunięcie celu, ani dopełnienie. Test na ustalonej stronie pokazał, że `scroll_target` stawia sekcję na 96 px, a pod ostatnią sekcją zostaje 48 px (`PAGE_PADDING_BOTTOM` + `VIEW_PADDING_BOTTOM` są stosowane). Błąd był w chwili skryptowanego kliknięcia scenariusza: widok klikał w pierwszej klatce, w której sekcje miały granice, czyli przed końcem pracy startowej. Potem karta skanu rosła o 18,5 px (linia narzędzi wideo +36, linia „skanowanie startowe” −16, pusta lista rootów → wiersz −1,5), a przesunięcie zostawało: 96 + 18,5 = 114,5 px w settings-appearance-language i 48 − 18,5 = 29,5 px w settings-danger-about. `Runtime::scripted_settings_section` zwraca teraz sekcję dopiero po skanie startowym z odświeżeniami, liście rootów i odczycie narzędzi wideo, a widok klika klatkę po ułożeniu tej strony. Zrzuty: karta Appearance na y 96 i karta About do y 851 w obu aplikacjach.
  - G4 (skan startowy): luka była prawdziwa. Runtime odświeżał bibliotekę bez biletu (`LibraryInput::Refresh { ticket: None }`), więc błąd słownika tagów nie był w ogóle widoczny, a wynik był zawsze podsumowaniem. Skan startowy kończy teraz runner ustawień (`Settings::startup_scanned`) tą samą ścieżką co ręczny rescan: po skanie z pracą nieudane odświeżenie biblioteki albo listy rootów staje się błędem linii skanu z prefiksem rescanu, po nieudanym skanie odświeżenia też idą, a zostaje błąd skanu (`Promise.allSettled`). `Settings::finish_startup` i `STARTUP_ERROR_PREFIX` zniknęły. `StartupScanOutcome` dalej opisuje sam skan (`Scanned` także wtedy, gdy odświeżenie padło).
  - G5 (stara lista rootów): `SettingsEffect::Roots` anuluje teraz oczekujące listowanie runtime'u (`scan_roots_request.cancel()`), więc spóźniona odpowiedź listowania startowego nie nadpisuje listy opublikowanej przez operację.
  - G6 (fokus po aktywacji linku): sekcje mają uchwyty fokusu bez przystanku Tab (`SettingsView::section_focus`), a `activate_section` przenosi fokus na sekcję i dopiero przewija. Następny Tab trafia w pierwszą kontrolkę sekcji. Pierścień fokusu sekcji (kwadratowy, jak `:focus-visible` na `.settings-section`) rysuje się tylko przy klawiaturze; skryptowane kliknięcie scenariusza przenosi fokus bez pierścienia.
  - G7: `SettingsView::remove_dialog_path` (tylko odczyt) i test, że ścieżka zostaje od zamknięcia do końca wygaszania, a kolejne pytanie pokazuje własny root. Zachowanie bez zmian.
  - G8: selektory debug znacznika rozwinięcia i rozwiniętego tekstu (`legal_marker_selector`, `legal_text_selector`) oraz test ścieżki wskaźnika: klik rozwija, drugi zwija, każde podsumowanie osobno, znacznik zgodny ze stanem.
  - Dokumenty: `settings-operations.md` („GPUI port: settings”), `scanning-and-indexing.md`, `architecture-and-ui-conventions.md` (runtime, klawisze), `testing.md` (wiersze testów, scenariusze `settings-`).
- Odkryte:
  - Skrypt scenariusza wołał `script_settings` przed `RuntimeInput::Start` (src-gpui/crates/media_tagger_gpui/src/window.rs:321), więc strona ustawień rysowała się, zanim odpowiedziało cokolwiek ze startu. Każdy przyszły scenariusz, który klika w ułożenie strony ustawień, musi czekać tak jak `scripted_settings_section` (dotyczy scenariuszy E18).
  - `begin_startup` zwraca `None`, gdy przy starcie runner trzyma inna operacja. Ta ścieżka została: runtime odświeża wtedy sam i nie rusza linii sekcji (`startup_refresh_own`, test `a_startup_scan_that_cannot_take_the_runner_is_refreshed_by_the_runtime`).
  - Kolejność Tab na stronie ustawień różni się od Tauri: nawigacja jest rysowana po sekcjach (src-gpui/crates/media_tagger_gpui/src/views/settings.rs, `render`: `header`, `body`, `nav`), więc Tab idzie nagłówek → kontrolki sekcji → linki nawigacji, a w Tauri nagłówek → linki → sekcje. Sprawdzone sondą: Tab z podsumowania „Privacy notice” trafia w link „Scan settings”. Nie naprawione (poza listą luk); zapisane w E19.
  - Krawędź aktywnego linku nawigacji: w Tauri `box-shadow: inset 2px 0` idzie po zaokrągleniu 6 px, w GPUI to prosty pasek 2 px (maska `overflow_hidden` jest prostokątna). Różnica rzędu 1–2 px na rogach, zostawiona.
  - Strzałka „Back” w nagłówku ustawień (E06, `settings_header::BACK_ICON_SIZE` = 16) ma tę samą przyczynę co G1: w Tauri rysuje się w 20 px. Nie ruszane (nagłówek należy do E06); zapisane w E19.
- Zostało: nic z G1–G8. Do decyzji przy domknięciu (E19): kolejność Tab nawigacji, strzałka „Back” i pozostałe różnice nagłówka ustawień (tytuł ok. 6 px w lewo, dolna krawędź na y 74 zamiast 72). Bez zmian zostają rzeczy wyłączone z audytu: rasteryzacja fontów (D10), pusty pas sekcji E18, tytuł dialogu folderu, brak awaryjnego adresu po nieudanym otwarciu linku, brak podpowiedzi. Nie sprawdzono na prawdziwym ekranie: pierścienia fokusu sekcji po Enterze na linku (tylko test okna na platformie testowej GPUI) i zachowania z czytnikiem ekranu.
- Testy:
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --test settings_ported` → pass (44; 4 nowe: trzy przypadki odświeżeń po skanie startowym i spóźnione listowanie startowe; ten ostatni bez poprawki G5 → fail)
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --lib runtime::tests` → pass (29; 1 nowy)
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test settings_window` → pass (14; 6 nowych: margines przewijania, dół strony, wymiary nawigacji, fokus sekcji, ścieżka w wygaszanym dialogu, teksty prawne wskaźnikiem)
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test settings_ported` → pass (13; test scenariuszy z nowymi asercjami geometrii przed poprawką → fail, 114,5 px zamiast 96 px)
  - `job.py start E17 compare --scenario settings-scan`, `settings-appearance-language`, `settings-danger-about` (po kolei) → pass, zrzuty zmierzone
  - `job.py start E17 checks` → pass (8/8 kroków)
- Uzupełnienie (G9, ta sama sesja): kolejność Tab jest naprawiona i nie zostaje do E19, wbrew punktom „Odkryte” i „Zostało” wyżej. Nawigacja jest teraz rysowana przed kontenerem sekcji (`views/settings.rs`, `render`: `header`, `nav`, `body`), a GPUI układa przystanki w kolejności rysowania, więc Tab idzie nagłówek → linki nawigacji → kontrolki sekcji, a Shift+Tab tą samą drogą wstecz (z pierwszej kontrolki sekcji i z sekcji z fokusem po aktywacji linku: do ostatniej kontrolki poprzedniej sekcji). Zrzuty GPUI trzech scenariuszy są po zmianie identyczne co do piksela (0 różnych pikseli; w settings-scan poza ścieżką roota, która zmienia się z każdym uruchomieniem). Testy: `settings_window` → pass (15; nowy `tab_goes_through_the_header_the_navigation_and_then_the_sections`, przy starej kolejności → fail), `job.py start E17 compare` ×3 → pass, `job.py start E17 checks` → pass (8/8 kroków).

### 2026-11-23 — orkiestrator (zamknięcie)

- Zrobione: audyt po workflow. Zakres review zgodny w każdej rundzie (40, 43, 43, 43 pliki); każda soczewka ma wynik na pełnym zakresie (frontend, performance i tooling w rundach 1–2, conventions, rust i gpui w rundach 1–4). Braki z audytu G1–G9 zamknął jeden agent poprawkowy (wpis wyżej): wymiary nawigacji, moment skryptowanego kliknięcia scenariuszy, błąd odświeżenia po skanie startowym, spóźnione listowanie rootów, fokus sekcji po aktywacji linku, kolejność Tab, trzy brakujące testy.
- Odkryte: notatka workflow jest zgodna z `checks.steps.json` (8/8 kroków, kod 0). Punkt (8) tej notatki o różnicach parity jest nieaktualny po poprawkach: zostaje tylko pusty pas sekcji E18 i nagłówek ustawień z E06 (zapisany w E19).
- Zostało: nic otwartego w E17. Poprawki po audycie nie przeszły ponownie przez soczewki review; mają testy regresji i zieloną bramkę. Nie sprawdzono na prawdziwym ekranie: dialogu folderu przez portal XDG, pierścienia fokusu sekcji, kolejności Tab.
- Testy: `job.py start E17 checks` → pass (8/8); `job.py start E17 compare` dla `settings-scan`, `settings-appearance-language`, `settings-danger-about` → pass, zrzuty obejrzane (nawigacja, karta Appearance i dół strony zgodne z Tauri); `job.py start E17 test-all` → pass (`[E2E] COMPLETE PASS`, exit 0).
