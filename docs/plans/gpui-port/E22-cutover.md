# E22: Przełączenie: identyfikator wydania, pakowanie, usunięcie Tauri

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E19, E21.

**Odroczony.** Nie uruchamiać workflow bez nowej decyzji użytkownika; przed startem powstaje osobny plan tego epiku.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Zastąpić aplikację Tauri aplikacją GPUI.

## Zakres

- Osobny plan i ADR: identyfikator wydania i migracja danych profilu, pakowanie (Flatpak/AppImage/native), usunięcie frontendu Solid i adaptera Tauri, E2E dla GPUI.

## Poza zakresem

- brak

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `packaging`
- `src-tauri/tauri.conf.json`

Testy do przeniesienia:
- brak

Dokumenty i reguły:
- `docs/development/linux-packaging.md`

## Kryteria akceptacji

- Do ustalenia w planie epiku.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E22 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`

## Scenariusze porównania

- brak (epik bez widocznego odpowiednika w Tauri albo bez UI)

## Ryzyka

- Migracja danych użytkownika.

## Status

deferred

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — agent naprawczy luk E06

- Z epiku E06: GPUI przypiętego commitu rysuje zmienny font tylko w jego domyślnej instancji (waga 400) i nie ma odstępów liter. Dołączony `public/fonts/Inter.ttf` jest zmienny, więc w pakiecie albo na systemie bez zainstalowanego Inter wszystkie wagi inne niż 400 (nagłówki 650, logotyp i przyciski 600) rysują się jako 400; z systemowym Inter wagi biorą najbliższą statyczną twarz. Przed przełączeniem potrzebna decyzja: statyczne instancje Inter jako zasoby GPUI albo nowsza rewizja GPUI z obsługą wariacji. Szczegóły: `reports/E06-review-open.md` (font-2, font-3).

### 2026-11-24 — implementer E19

- Z epiku E19 (stan przy domknięciu portu): port obejmuje wszystkie ekrany aplikacji Tauri poza animacją GIF (E20) i odtwarzaniem wideo (E21). Pełny przebieg 23 scenariuszy porównania nie wykazał różnic blokujących; werdykty są w notatkach E19, a lista różnic świadomych w `docs/frontend/gpui-architecture.md` („Parity with the Tauri interface”, „Keyboard and focus”). Poniżej braki, które nie są rasteryzacją fontów i które plan przełączenia musi rozstrzygnąć.
- Z epiku E19, braki funkcjonalne względem Tauri:
  - paski przewijania poza galerią: strona ustawień, lista tagów, panel boczny lightboxa, lista resolvera duplikatów i teksty licencji przewijają się kółkiem, klawiszami i linkami nawigacji, ale nie pokazują pozycji (GPUI nie ma paska; galeria ma własny, ADR 0042);
  - podpowiedzi `title` (ok. 30 miejsc w Solid: przyciski z samą ikoną, obcięta nazwa pliku i ścieżka, stan tagu na liście tagów, podpowiedź składni wyszukiwania); GPUI ma `tooltip`, port go nie używa;
  - regiony `aria-live` (ogłoszenie przesunięcia kafla `bulk.orderModal.moved`, regiony statusu): przypięte GPUI ich nie ma; role i nazwy dostępne są ustawione w kodzie, ale nie były sprawdzane czytnikiem ekranu;
  - obraz ponad budżet dekodowania pokazuje błąd mediów zamiast pomniejszonego obrazu (ADR 0046; wymaga zależności `image` w adapterze), mocno pomniejszony obraz nie ma mipmap;
  - nazwa pliku eksportu bierze czas lokalny z polecenia `date`, UUID nie ma źródła kryptograficznego, a przywracanie czekające na potwierdzenie jest porzucane przy wyjściu ze strony ustawień (trzy decyzje zostawione użytkownikowi w E18).
- Z epiku E19, fonty: bez zainstalowanego statycznego Inter wszystkie wagi rysują się jako 400, brak odstępów liter (notatka E06 wyżej). To największa widoczna różnica w nagłówkach i przyciskach; wymaga statycznych instancji Inter jako zasobów albo nowszej rewizji GPUI.
- Z epiku E19, drobne różnice układu zostawione bez poprawki: 1 px na polach o ułamkowej pozycji (wiersz narzędzi galerii, wiersze sugestii, karta „Danger zone”, karty resolvera), pierścień fokusu `ui::theme::focus_ring` o 1 px bliżej na elementach z obwódką (kompensuje to tylko pole nazwy resolvera), krycie tekstu wyłączonego pola nazwy (0,30 zamiast 0,22), kolejność Tab w lightboxie (pływające przyciski po szynie akcji), klik w puste miejsce doku nie zdejmuje fokusu z pola.
- Z epiku E19, niesprawdzone na prawdziwym ekranie (wszystkie przebiegi szły na Xvfb albo na platformie testowej GPUI): pełny ekran na kompozytorze Wayland i menedżerze okien X11 (termin 3 s na odpowiedź okna), dialogi plików portalu XDG (bez tytułu i filtra plików), szybkość zoomu kółkiem prawdziwej myszy, wygląd najechania wskaźnikiem, metoda wprowadzania (IME), dekoracje okna przy menedżerze, który zostawia własną belkę.
- Z epiku E19, wydajność: pomiar na 1 000 000 pozycji (`tests/million.rs`, wyniki w notatkach E19 i w `docs/frontend/gpui-architecture.md`) idzie na platformie testowej GPUI: układ i malowanie sceny bez GPU. Nie ma pomiaru klatki z rasteryzacją na prawdziwym GPU, pamięci prawdziwej binarki na ekranie ani pomiaru całej aplikacji Tauri na tej samej bazie (`app:control` nie ma polecenia wykonującego skrypt w oknie); po stronie Tauri porównanie obejmuje wywołania rdzenia (`grid_baseline.rs`) i osobny fixture frontendu (`e2e/gallery-scale`).
- Z epiku E19, narzędzia: okno GPUI nie ma sterownika wejścia (`gpui:control` tylko uruchamia stan startowy), więc desktop E2E dla GPUI trzeba zaprojektować od zera; porównanie osi czasu wymaga seedu wielomiesięcznego i scenariusza `gallery-timeline` (propozycja w `reports/E10-review-open.md`); otwarte ustalenia P2/P3 z przeglądów leżą w `reports/` (E05–E08, E10, E12, E13).
- Z epiku E19, instrukcje agentów: implementer E19 nie zmieniał `CLAUDE.md` ani `.claude/rules/` (wymaga to zgody użytkownika, nie polecenia workflow). Do dopisania: wiersz indeksu dokumentacji dla `docs/frontend/gpui-architecture.md`, wskazanie tej strony w `.claude/rules/gpui-port.md` i `.claude/rules/frontend.md`, komenda pomiaru `--test million` w sekcji komend. `.claude/skills/audit/SKILL.md` podaje „about 65 finder agents”; po dodaniu batchy GPUI pełny przebieg ma 95 finderów (`docs/development/audit/README.md` jest zaktualizowany).

### 2026-11-24 — poprawki po audycie orkiestratora (E19)

- Z epiku E19 (sprostowanie wpisu implementera wyżej, punkt „drobne różnice układu”): cztery z wymienionych tam różnic zostały poprawione w E19 i nie należą już do E22: pierścień fokusu na elementach z obwódką (`ui::theme::focus_ring` bierze obwódki elementu, `RingBorder`), krycie tekstu wyłączonego pola nazwy resolvera (`FieldChrome::Framed`, 0,22), kolejność Tab w lightboxie (grupy Tab, kolejność DOM) i fokus po naciśnięciu pustego miejsca doku. Zostaje z tego punktu tylko 1 px na polach o ułamkowej pozycji. Doszła jedna drobna różnica: naciśnięcie na chip tagu w edytorze tagów zostawia fokus tam, gdzie był (`docs/frontend/gpui-architecture.md`, „Keyboard and focus”).
- Z epiku E19: punkt „instrukcje agentów” z wpisu implementera pozostaje otwarty i czeka na zgodę użytkownika (`CLAUDE.md`, `.claude/rules/gpui-port.md`, `.claude/rules/frontend.md`, `.claude/skills/audit/SKILL.md`). Do listy doszły: aktualne moduły obu crate'ów GPUI w bloku „Layout” i opis celów testowych przy `bun run test:gpui`.
