# E18: Ustawienia: import/eksport, przywracanie i duplikaty

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E17.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Operacje na danych z tymi samymi zabezpieczeniami co w Tauri.

## Zakres

- Eksport/import CSV tagów; kopia i przywracanie paczki bazy: inspekcja paczki, mapowanie rootów, potwierdzenie nadpisania; reset stanu tagów po przywróceniu.
- Duplikaty: skan, dialog rozstrzygania (karty grup i zasobów), walidacja, zastosowanie partii z `scanRevision`, potwierdzenie usunięcia.

## Poza zakresem

- brak

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/settings/createImportExportSettings.ts`
- `src/features/settings/ImportDbOverwriteConfirmDialog.tsx`
- `src/features/settings/createDuplicateSettings.ts`
- `src/features/settings/createDuplicateResolver.ts`
- `src/features/settings/duplicateValidationService.ts`
- `src/features/settings/DuplicateResolverDialog.tsx`
- `src/features/settings/DuplicateGroupCard.tsx`
- `src/features/settings/DuplicateAssetCard.tsx`
- `src/features/settings/DuplicateDeleteConfirmDialog.tsx`
- `src/features/settings/sections/ImportExportSection.tsx`
- `src/features/settings/sections/DuplicatesSection.tsx`

Testy do przeniesienia:
- `src/features/settings/__tests__`

Dokumenty i reguły:
- `docs/subsystems/data-safety-and-portability.md`
- `docs/subsystems/settings-operations.md`
- `.claude/rules/data-safety.md`

## Kryteria akceptacji

- Testy modeli importu/eksportu i duplikatów przeniesione.
- Każdy test niszczący działa na świeżym katalogu tymczasowym i tymczasowej bazie.
- Przywracanie i import CSV przechodzą przez barierę mutacji tagów.
- Parytet `settings-import-export`, `settings-duplicates`, `duplicates-resolver`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E18 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `settings-import-export`: Sekcja importu/eksportu.
- `settings-duplicates`: Sekcja duplikatów po skanie z wynikami.
- `duplicates-resolver`: Dialog rozstrzygania duplikatów z jedną grupą.

## Ryzyka

- Najwyższe ryzyko utraty danych w całym porcie: przegląd z soczewką stabilności obowiązkowy.

## Status

done 2026-11-24

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-24 — workflow gpui-port-epic
- Zrobione: model klienta (`media_tagger_client::settings`): eksport/import CSV, eksport kopii, przywracanie jako dwie operacje z potwierdzeniem nadpisania pomiędzy nimi, skan duplikatów, stan resolvera, potwierdzenie usuwania i jedna paczka ze zmianami (rewizja skanu, wersje ścieżki/rekordu). Import CSV i potwierdzone przywracanie przechodzą przez `BeginMaintenance`/`EndMaintenance` (bariera mutacji). Czysta logika w `settings/duplicates.rs` i `settings/transfer.rs`. Adapter: sekcje importu/eksportu i duplikatów, dialog resolvera (wirtualna lista, pola zmiany nazwy, miniatury), potwierdzenia usuwania i nadpisania, systemowe dialogi plików. Scenariusze `settings-import-export`, `settings-duplicates`, `duplicates-resolver` w trzech miejscach oraz nowy seed harnessu `duplicates`. Dokumentacja: settings-operations, data-safety-and-portability, testing, architecture-and-ui-conventions. Testy: 26 nowych (18 klienta, 8 adaptera), razem 43 przypadki TS uznane za przeniesione. Mutacje implementacji (6 + 2) były wykrywane przez testy.
- Odkryte: (1) Seed `duplicates` to rozszerzenie harnessu poza pliki scenariuszy: compare.js zapisuje `copy/control-1.png` do katalogu mediów sesji app:control (strażnicy: jedyny root sesji, pod katalogiem tymczasowym, dokładnie seed control); do potwierdzenia przez orkiestratora. (2) Dialogi plików należą do adaptera (`PathDialogRequest`, `SettingsInput::PathPicked`); GPUI nie ma tytułu ani filtra plików. (3) Odstępstwo od Solid: przy dialogu wyboru folderu dla roota status to `Choose folder: <root>`. (4) Nazwa eksportu z czasem lokalnym z polecenia `date` (fallback UTC); UUID v4 z kluczy haszujących biblioteki standardowej (bez nowych zależności, nie kryptograficzny). (5) Formularz resolvera należy do widoku ustawień i resetuje się przy otwarciu lub zmianie `duplicate_scan_generation`. (6) Po paczce duplikatów odświeżenie biblioteki i rescan idą równolegle (best effort). (7) Błędy walidacji przy zapisie to klucze lokalizacji (zmieniają się z językiem). (8) `Thumbnails::sync_resolver` dodaje podglądy resolvera do wspólnego zapotrzebowania. (9) Pola nazw używają `FieldChrome::Outlined`. (10) Zmienione oczekiwania istniejących testów: kolejność tab w `tests/settings_window.rs`, lista scenariuszy w `tests/scenario.rs`, listy seedów/scenariuszy w e2e/compare; w README planu tylko lista seedów. (11) Test rzeczywistego rdzenia przywracania nie dowodzi bariery (zostaje zielony bez niej); barierę dowodzą testy skryptowe. (12) Pominięte przypadki TS: pomiar wierszy wirtualnej listy DOM, początkowy fokus na „Discard changes”, teksty „Pending: delete file”/„Unmark delete”, tytuł/filtr dialogu platformy, restore z mapowaniem rootów na prawdziwym rdzeniu (rdzeń ustawia requires_mapping tylko dla rootów Windows), przypadki należące do E14–E17. (13) Do E17/E19: klik „Danger zone” zaznacza Danger, a strona jest na końcu (Tauri prawdopodobnie zaznacza About; zmierzono tylko GPUI). (14) Różnica parytetu poniżej progu (minor): przycisk Wstecz i logo w nagłówku ustawień (ikona ok. 12 px vs 16 px, tytuł ok. 6 px dalej w lewo); własność E17/D14. (15) Zdjęcia E17 się zmienią, bo strona jest dłuższa; ponownie nie porównywano. Miejsca najbardziej prawdopodobnych porażek pierwszego compare:ui: krok rescan czeka na tekst zaczynający się od `Done. Indexed:`; `settings-import-export.js` czeka na `aria-current="location"`; scenariusze duplikatów celowo nie czekają na aria-current; `controlMediaRoot` opiera się na kształcie `roots` w READY.
- Zostało: nie uruchomiono `compare:ui` dla trzech scenariuszy; parytet jest sprawdzony tylko w jednej rundzie wizualnej (bez otwartych różnic poza wskazaną mniejszą), a strona Tauri nowych kroków i seedu nie była nigdy uruchomiona na prawdziwej sesji app:control. Nieprzeniesione testy TS: pluralCopy (plurale potwierdzenia usuwania i ostatniego wyniku), confirmationPresence, część DuplicateResolverDialog (fokus/przewijanie), pozostałe przypadki duplicateValidationService. Niezweryfikowane: dialogi XDG na prawdziwym ekranie, nazwa pliku eksportu, pisanie w polu zmiany nazwy w oknie, układ jednokolumnowy poniżej 768 px, jasny motyw, render pikseli nowych sekcji. Przegląd: 3 rundy, 8 ustaleń potwierdzonych i naprawionych, brak otwartych (brak raportu); luka: runda 1 review-tooling zwróciła pusty wynik. Bramka: zaliczona w 1 rundzie.
- Testy: `bun run rust:fmt` pass; `bun run rust:clippy` pass; `architecture_boundaries` pass (33); `cargo fmt` src-gpui pass; `cargo clippy` src-gpui pass; `bun run gpui:lock-parity` pass; `cargo test -p media_tagger_client` pass (1309); `cargo test -p media_tagger_gpui` pass (401); `bun run test:compare` pass (58); `bun run test:all` (bramka) pass; `compare:ui` nieuruchomione.

### 2026-11-24 — poprawki po audycie orkiestratora (E18)
- Zrobione:
  - G1 (aktywna sekcja na końcu strony): prawdziwe. `SettingsNavigation.tsx` ma dwa handlery: klik robi `scrollIntoView` i `setActiveSection(id)`, a zdarzenie `scroll`, które idzie po kliknięciu, które przesunęło stronę, zaznacza od nowa według pozycji (`update`), jeszcze przed narysowaniem klatki. Sekcja, która nie dochodzi do marginesu przewijania, bo strona kończy się wcześniej („Danger zone” w oknie 900 px), zostawia więc zaznaczone About (koniec strony); klik, który nic nie przesunął (strona już jest na końcu), nie wywołuje `scroll` i zostawia zaznaczoną klikniętą sekcję do następnego przewinięcia albo zmiany rozmiaru okna. GPUI trzymało klikniętą sekcję zawsze do następnego przewinięcia. Nowe `settings::navigation::NavigationMark` (klient) odwzorowuje oba handlery, widok trzyma je zamiast pary `clicked`, a zmiana rozmiaru okna czyści znacznik (listener `resize`). Stanu pośredniego „zaraz po kliknięciu” GPUI nie ma: przesunięcie jest synchroniczne, więc od razu widać to, co Tauri rysuje po zdarzeniu `scroll`. Solid nie ma testu tego komponentu; pomiar na zrzutach Tauri potwierdza regułę dla sekcji dochodzących do marginesu (Import/Export, Duplicates, About).
  - G2 (panika wywołania backendu): nieprawdziwe. `exec/thread.rs` tylko chroni wątek puli; odpowiedź daje `Dispatch::call` (`model.rs`), które łapie panikę w zadaniu i odsyła błąd `WorkerFailed`, a zadanie porzucone przez wykonawcę odpowiada przez strażnika `Answer`. Każda gałąź `SettingsCall` idzie przez `Dispatch::call` (`runtime.rs`, `run_settings_call`), więc operacja kończy się zwykłą ścieżką błędu. Dopisane testy to potwierdzają dla operacji bez bariery (eksport CSV, skan duplikatów z subskrypcją postępu) i za barierą (import CSV, przywracanie, czyszczenie biblioteki): linia błędu, zwolniona blokada, `EndMaintenance` z resetem, odświeżenie, a zapis tagów jest znów przyjmowany.
  - G3 (zaległe przywracanie): prawdziwe dla drugiego wyboru archiwum. `pick_bundle` zostawiał `pending_import` pierwszego archiwum; po anulowanym albo nieudanym drugim wyborze dało się potwierdzić pierwsze (tak samo zachowuje się kontroler Solid, gdzie stan jest nieosiągalny, bo dialog zakrywa stronę). Start drugiego wyboru czyści teraz zapamiętane przywracanie od razu. Cancel, Escape, tło i Confirm czyściły je już wcześniej, a odrzucony wybór albo Cancel w trakcie innej operacji zostawiają je bez zmian (jak `runExclusiveOperation`). Opuszczenie strony też je teraz czyści (`Settings::page_left` przy `RuntimeInput::CloseSettings`), z linią „cancelled”.
  - G4 (podglądy resolvera po opróżnieniu listy): prawdziwe. `schedule_preview_flush` wołały tylko rysowane wiersze, więc klatka bez kart nie oddawała zapotrzebowania (`Thumbnails::sync_resolver`) i podglądy poprzednich grup zostawały zamówione i przypięte do zamknięcia dialogu. Teraz o flush prosi sam otwarty dialog, raz na klatkę.
  - G5 (Escape): zachowanie było poprawne, brakowało testu. Nowy test okna: Escape zamyka dialog z wierzchu i nic pod nim (potwierdzenie usuwania nad resolverem, resolver nad stroną, potwierdzenie nadpisania nad stroną), w trakcie operacji nie zamyka ani dialogu, ani strony, klik w miejscu przycisku Back pod potwierdzeniem trafia w tło dialogu, a bez otwartego dialogu Escape zamyka stronę, także w trakcie operacji (`App.tsx` rejestruje `closeSettings` bez sprawdzania blokady).
  - G6 (render nowych widoków): `tests/settings_render.rs` rysuje sekcję import/eksport, sekcję duplikatów z linią wyniku oraz resolver z jedną grupą dwóch plików, w spoczynku i z fokusem w polu nazwy, w obu motywach (PNG przy `MEDIATAGGER_GPUI_SCREENSHOT_DIR`); obrazy obejrzane, jasny motyw bez uwag. Trzy różnice miar naprawione: (a) nagłówek dialogu miał linię 28 px (`text-lg`), a `h3` bez klasy rozmiaru ma 18 px na linii 1,5, czyli 27 px (`layout::dialog`; dotyczy wszystkich pięciu dialogów ustawień); (b) pole nazwy rysowało tekst 13 px w ramce filtra listy tagów (`FieldChrome::Outlined`: obwódka 12 %, pełne tło, cień), a klasy karty `input input-sm h-10 border border-base-content/20 bg-base-100/70` dają ramkę daisyUI: tekst 12 px, obwódka 20 %, tło 70 %, bez cienia, przy fokusie obrys 2 px w kolorze treści 2 px od obwódki. Pole jest teraz gołym `TextInput` w ramce rysowanej przez kartę (`SettingsView::rename_field`, `layout::resolver::INPUT_*`); wspólny prymityw bez zmian; (c) przez (a) cała treść dialogu leżała 1 px niżej, a przez (b) tekst pola kolejne 2 px niżej.
  - Dokumenty: `settings-operations.md` („GPUI port: settings”: przywracanie, panika wywołania, podglądy, pole nazwy, nawigacja, Escape), `data-safety-and-portability.md`, `testing.md` (wiersze testów, nowe zrzuty).
- Odkryte:
  - Po poprawkach zrzut `duplicates-resolver` (1440x900): nagłówek, przyciski nagłówka, linie statusu i podpowiedzi, góra karty grupy (y 194) i pierwszej karty pliku (y 234), trzy linie tekstu karty i tekst w polu nazwy leżą w tych samych wierszach pikseli w obu aplikacjach; kolory pola są identyczne (obwódka 73,75,76, tło 30,33,34). Zostaje 1 px na ramce pola, wierszu przycisków i dolnych krawędziach kart (GPUI y 323/362/415/424/618, Tauri 322/361/414/423/617). Przyczyna jest subpikselowa: linia nazwy grupy (`text-sm`) ma 18,571 px, więc karta zaczyna się na 233,571; GPUI zaokrągla każdą krawędź do najbliższego piksela, WebKit rysuje górę karty na 234, a pudełka w środku piksel wyżej. Nie ruszane.
  - `ui::theme::focus_ring` liczy pierścień od pudełka wypełnienia rodzica (pozycjonowanie absolutne), więc na elemencie z obwódką leży 1 px bliżej niż obrys CSS liczony od krawędzi obwódki. Pole nazwy kompensuje to lokalnie (`duplicates.rs`, `rename_field`); inne miejsca użycia na elementach z obwódką nie były sprawdzane.
  - Wyłączone pole nazwy: ramka ma krycie 0,55 jak `input:disabled`, tekst wychodzi na ok. 0,30 (pole i ramka przyciemniają osobno), a w Tauri na ok. 0,22 (daisyUI daje jeszcze 40 % koloru tekstu). Bez zrzutu porównawczego.
  - Karta „Danger zone” (E17) jest w GPUI o 1 px niższa niż w Tauri (114 zamiast 115 px wnętrza; linia 19,5 px nagłówka `text-base`), więc About zaczyna się piksel wyżej. Widoczne na zrzutach `settings-duplicates` i `settings-import-export`; poza zakresem tego audytu.
  - Opuszczenie strony z czekającym przywracaniem różni się od kontrolera Solid, którego stan przeżyłby stronę i otworzył potwierdzenie przy następnym wejściu. W obu aplikacjach stan jest nieosiągalny z klawiatury i wskaźnika (dialog zakrywa stronę i bierze Escape); potwierdzenie nie pokazuje ścieżki archiwum, więc w GPUI nie wraca po ponownym otwarciu strony.
  - Pozostałe modele (lightbox, dok, biblioteka, lista tagów) wołają backend przez to samo `Dispatch::call`; jedyne `Dispatch::spawn` to tor miniatur (`thumbs.rs`), który łapie panikę sam. Luki z G2 tam nie ma.
- Zostało: nic z G1–G6. Nie sprawdzono na prawdziwym ekranie: obrysu fokusu pola nazwy i pisania w nim poza platformą testową, dialogów XDG, układu jednokolumnowego poniżej 768 px. Poprawki nie przeszły ponownie przez soczewki review. Do domknięcia (E19): reszta 1 px w resolverze, `focus_ring` na elementach z obwódką, krycie tekstu wyłączonego pola, 1 px karty „Danger zone”.
- Testy:
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --lib settings::` → pass (nawigacja 8, w tym 4 nowe, z których 1 przy starej regule zaznaczania → fail; `settings::layout` 2 nowe: linia nagłówka dialogu i klasy pola nazwy w `DuplicateAssetCard.tsx`)
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --test settings_data` → pass (34; 5 nowych: drugi wybór archiwum, odrzucony wybór i Cancel, opuszczenie strony, panika bez bariery, panika za barierą; test drugiego wyboru bez poprawki G3 → fail)
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test settings_window` → pass (16; 1 nowy, przy starej regule zaznaczania → fail)
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test settings_data_window` → pass (12; 3 nowe: Escape, strona otwarta ponownie, ramka pola nazwy; rozszerzony test rescanu bez poprawki G4 → fail, 4 podglądy zamiast 0)
  - `MEDIATAGGER_GPUI_REQUIRE_RENDERER=1 cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test settings_render` → pass (2; 1 nowy), zrzuty obejrzane
  - `job.py start E18 compare --scenario` dla `settings-import-export`, `settings-duplicates`, `duplicates-resolver`, `settings-danger-about` (po kolei) → pass, zrzuty zmierzone
  - `job.py start E18 checks` → pass (8/8 kroków; `architecture_boundaries` 33, `media_tagger_client` 1340, `media_tagger_gpui` 416)

### 2026-11-24 — orkiestrator (zamknięcie)

- Zrobione: audyt po workflow. Zakres review zgodny w każdej rundzie (40 plików); soczewka review-tooling nie zwróciła wyniku w rundzie 1, ale przeszła pełny zakres w rundzie 2 bez ustaleń, więc luka z raportu workflow jest zamknięta. Sześć pozycji z audytu (G1–G6) zamknął jeden agent poprawkowy (wpis wyżej).
- Odkryte: sprostowanie notatki workflow. `compare:ui` dla trzech scenariuszy był uruchomiony w kroku stabilizacji (wynik ok, jedna runda parity, jedyna różnica to nagłówek ustawień z E06), a strona Tauri nowych kroków i seedu `duplicates` działała na prawdziwej sesji app:control. Przypadki `pluralCopy`, `confirmationPresence`, fokus/przewijanie resolvera i pozostałe przypadki `duplicateValidationService` zostały przeniesione w kroku testów (`settings_data_ported.rs` w obu crate'ach). Seed `duplicates` (zapis jednego pliku do katalogu mediów sesji, ze strażnikami) potwierdzam: przeszedł pięć soczewek bez ustaleń.
- Zostało: nic otwartego w E18. Poprawki po audycie nie przeszły ponownie przez soczewki review; mają testy regresji i zieloną bramkę. Nie sprawdzono na prawdziwym ekranie: dialogów plików portalu XDG, nazwy pliku eksportu, układu jednokolumnowego poniżej 768 px. Do decyzji użytkownika: czas lokalny w nazwie eksportu z polecenia `date`, UUID bez źródła kryptograficznego, czyszczenie oczekującego przywracania przy wyjściu ze strony (Solid by je zachował; stan nieosiągalny z UI).
- Testy: `job.py start E18 checks` → pass (8/8); `job.py start E18 compare` dla `settings-import-export`, `settings-duplicates`, `duplicates-resolver`, `settings-scan`, `settings-appearance-language`, `settings-danger-about` → pass, zrzuty obejrzane; `job.py start E18 test-all` → pass (`[E2E] COMPLETE PASS`, exit 0).
