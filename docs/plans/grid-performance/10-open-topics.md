# Etap 10: otwarte tematy po fali E

Po fali E (HEAD `76ff5b1`). Wykonuje orkiestrator. Źródło tematów: [07](07-verification-and-docs.md), sekcja „Otwarte tematy” 1–13.

Dokumentacja do przeczytania:
- `docs/development/testing.md` (`gallery-scale`, flagi `MEDIATAGGER_*`, izolacja E2E);
- `docs/adr/0001`–`0008`;
- notatki etapów 08 i 09.

## Cel

Zamknąć tematy, które nie wymagają decyzji (część A), i przygotować propozycje dla tematów, które jej wymagają (część B). Implementacja części B dopiero po decyzji użytkownika, każdy temat w osobnym commicie.

## Zakres

### Część A (bez pytania)

| Temat | Co | Wynik |
| --- | --- | --- |
| 9 | Powtórka obu biegów `grid-baseline.e2e.js` na bezczynnej maszynie, profil jak w „Przed” (XDG i `TMPDIR` na dysku pod `/var/tmp`, Xvfb 1440×1100) | wiersze (\*) w kolumnie „Po 08–09” w [07](07-verification-and-docs.md) |
| 12 | Pomiar 1M na desktopie bez miliona plików: `e2e/gallery-scale/million.tsx` (`GALLERY_MILLION=1`) | skok osi czasu i Ctrl+A przy 1M |
| 5 | Stały spec 100k: `e2e/specs/gallery-100k.e2e.js` (`MEDIATAGGER_GALLERY_100K=1`) | Shift-click i refresh głęboko w liście |
| 13 | Nowa łatka `05-selection-scan.patch` na aktualnym `GalleryGrid.tsx` (defekt dla `isSelected`) | `git apply --check` na HEAD |
| 7 | `alt` kafla = `file_name`, nie `preview_path` | test `GalleryTile.test.tsx` |

### Część B (propozycje, czekają na decyzję)

Tematy 1, 3, 6, 10 i 11. Propozycje są w notatce orkiestratora niżej.

### Odłożone bez zmian

- **2. HiDPI, drugi rozmiar miniatur:** odrzucone decyzją użytkownika w etapie 06.
- **4. Bramka wysokości gridu:** tylko po aktualizacji WebKitGTK (`GALLERY_HEIGHT_PROBE=1 sh e2e/gallery-scale/run.sh`).
- **8. Pojedyncza wolna próbka w `gallery.performance`:** obserwacja przy kolejnych biegach.

## Testy

- `GALLERY_MILLION=1 sh e2e/gallery-scale/run.sh` i `sh e2e/gallery-scale/run.sh` (regresja harnessu).
- `MEDIATAGGER_GALLERY_100K=1 bun run test:e2e:tauri --spec e2e/specs/gallery-100k.e2e.js` (prywatny Xvfb, XDG pod `/tmp/mediatagger-*`).
- `MEDIATAGGER_GRID_BASELINE=1 MEDIATAGGER_PERF=1 bun run test:e2e:tauri --spec e2e/specs/grid-baseline.e2e.js`, dwa biegi.
- Vitest: `GalleryTile.test.tsx` i katalog `src/features/gallery`.
- `git apply --check` dla wszystkich łatek evalu `review-changes`.

## Status

in progress — 2026-11-07 (część A gotowa razem z pomiarem tematu 9; część B czeka na decyzję)

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->

### 2026-11-07 — orkiestrator (część A)
- Zrobione:
  - **Temat 12:** `e2e/gallery-scale/million.tsx` (`million.html`, `GALLERY_MILLION=1`): produkcyjne `createLibrary`, `GalleryGrid` z `GalleryTimelineScrubber` i `createSelection` podłączone jak w `createAppRuntime`; transport syntetyczny (1M wyników, 120 miesięcy w sekcjach ≤ 5000, strona i sekcje po 12 ms). Każda próbka skoku startuje z nowej sesji (pusty cache). Opis w `testing.md` („Million-result timeline jump and select-all”).
  - **Temat 5:** `e2e/specs/gallery-100k.e2e.js` (`MEDIATAGGER_GALLERY_100K=1`, wpis w tabeli flag i sekcja w `testing.md`). Trzy przypadki: Shift-click 10 000 pozycji w okolicy indeksu 50 000 (dokładnie `MATERIALIZE_LIMIT`, jeden `get_asset_query_id_range`, zero stron), Shift-click 0 → 99 999 (zero odczytów ID), refresh przez Search na ~50 000 (jedno `start_asset_query` z `anchorOffset` = pierwszy widoczny indeks, strona kotwicy nie jest pobierana ponownie, 240 klatek bez placeholderów i bez przesunięcia widoku). 100k plików to nie problem: kopiowanie ~1–1,5 s (tmpfs), skan debug ~4–5 s, cały spec ~20–25 s, więc mniejszy wariant nie jest potrzebny.
  - **Temat 13:** nowa łatka `05-selection-scan.patch`: `selectedIndices` skanuje cały renderowany zakres (`isSelected` dla każdego indeksu i nowa tablica) przy każdej ewaluacji kafla, zamiast jednego `isSelected(index, id)` O(log r). Wygenerowana `git diff` w klonie (`--no-hardlinks`, katalog zadania), przechodzi typecheck i testy `GalleryGrid`/`createSelection` (defekt jest „cichy”). **Łatka `03-locale-missing` też nie nakładała się na HEAD** (stopka po etapach 04/06), więc wygenerowałem ją ponownie z tym samym defektem (brak klucza w `pl.json`); inaczej `base_commit` 76ff5b1 w `expected.json` byłby nieprawdziwy. README evalu: opis `05` i instrukcja `git apply --check` przed biegiem. Wszystkie 6 łatek: `git apply --check` na HEAD → ok.
  - **Temat 7:** `GalleryTile` ma `alt={file_name}`. Selektory E2E (`img[alt$=".png"]`, `img[alt$=".gif"]`) dalej pasują, recepty `verify-mediatagger` nie używają `alt`, `lightbox-recovery` sprawdza `alt` obrazu lightboxa, nie kafla. `library-query-and-gallery.md` poprawiony.
- Odkryte:
  - Tauri nie pozwala podmienić `window.__TAURI_INTERNALS__.invoke` (przypisanie nic nie zmienia), więc spec liczy IPC przez opakowanie `fetch` dla `ipc:`, jak `grid-baseline`.
  - WebKitGTK podaje `event.timeStamp` klawiszy XTest w innej skali niż `performance.now()` (wychodziły ujemne czasy). Harness znaczy czas we własnym nasłuchu `keydown` (capture).
  - Refresh na indeksie 49 987: odpowiedź niesie stronę 49 920, a widok i overscan sięgają strony 50 048, która jest pobierana (1 IPC). Pomiar etapu 04 („zero stron”) zależał od położenia kotwicy względem granicy strony; spec dopuszcza najwyżej dwie sąsiednie strony.
  - **Temat 9 nie jest zrobiony.** O 20:58 na hoście działała gra pod Wine (`GameThread` ~645% CPU, load 8), więc nie mierzyłem. O 21:11 host był bezczynny (load 1,9) i uruchomiłem oba biegi, ale gra wystartowała ponownie ok. 21:12 (`GameThread` 625%, load 18–21). Oba biegi przeszły (3/3), lecz szły pod obciążeniem (`render_all_thumbnails` 118–122 s zamiast ~79 s), więc wierszy (\*) w 07 nie zastąpiłem.
- Zostało: temat 9 (powtórka na bezczynnej maszynie); część B czeka na decyzję. Commit części A czeka na akceptację.
- Testy:
  - `GALLERY_MILLION=1 sh e2e/gallery-scale/run.sh` → ok w trzech biegach. End → kafle najstarszego miesiąca z danymi: 21–27 ms (zawsze 1 strona spoza cache), Home 5–19 ms; Ctrl+A → licznik 1 000 000: 1–3 ms, 0 odczytów ID, `ids.size` 0. Pierwszy bieg pod obciążeniem gry, kolejne na wolnym hoście; wyniki w tym samym przedziale.
  - `sh e2e/gallery-scale/run.sh` (regresja po zmianie `webkit.py`) → ok; `bunx tsc --noEmit -p e2e/gallery-scale/tsconfig.json` → pass.
  - `gallery-100k.e2e.js` → 3/3 w dwóch biegach (wcześniejsze biegi padały na błędach speca: brak `return`, podmiana `invoke`, asercja „zero stron”). Shift-click 10k 10–11 ms (1 binarny odczyt), 100k 2–3 ms (0 odczytów), refresh: 1 start, strony `[50048]`, 0 klatek z placeholderami. Biegi na wolnym hoście (load 2–6).
  - `grid-baseline.e2e.js` → 3/3 w dwóch biegach, pod obciążeniem (wyżej); surowe wyniki w ignorowanym `artifacts/grid-baseline/`.
  - `bun run test` → 846/846 (w tym nowy `GalleryTile.test.tsx`: 2 z 3 przypadków padają na starym kodzie); typecheck, lint, format:check, locale:check → pass; `git diff --check` czysto, `--numstat` identyczny z `--ignore-cr-at-eol`; `privacy:check` i `test:privacy` → pass, nowe nieśledzone pliki przejrzane ręcznie.

### 2026-11-07 — orkiestrator (część B: propozycje)
- **Temat 1, scope asset protocol `"**"`.** Oryginały (lightbox, GIF) idą przez `convertFileSrc` (`src/platform/tauri/assets.ts`), wideo przez ID.
  - A. Dynamiczny scope: w konfiguracji `scope: []`, przy starcie i w `add_scan_root` `asset_protocol_scope().allow_directory(root, true)`, synchronizacja po restore. Tauri nie ma „unforbid”, więc usunięty root zostaje dozwolony do restartu (albo zablokowany na stałe do restartu po ponownym dodaniu). Koszt ~1 dzień.
  - B. Własny schemat `media://localhost/<asset_id>` (jak `thumb://`): ścieżka z bazy, tylko zindeksowane assety, bez `protocol-asset` i ścieżek z frontendu. Koszt ~2–3 dni (Rust, wrapper, lightbox, `ipc-contract.md`, `lightbox.md`, `system-overview.md`, test Rust + API + E2E `lightbox-images`/`lightbox-recovery`).
  - Rekomendacja: **B**, spójne z `thumb://` i wideo po ID, bez problemu forbid/unforbid. Ryzyko: regresje ładowania dużych obrazów w lightboxie, nagłówki cache. ADR w obu wariantach.
- **Temat 3, usunięcie `list_assets`.** Helper `e2e/assets.js` (`listAssetsForAssertions`): `start_asset_query` z `ownerId` `e2e-assertions`, strony po 256, `get_asset_details` dla `path`/`tags`/`size_bytes`; migracja 17 speców, potem komenda, wrapper, `contracts/`, `generate_handler!`, `ipc-contract.md`, funkcje i testy w `db/queries.rs`. Koszt ~1 dzień plus ~1–2 h biegów 17 speców. Ryzyka: start z innego ownera przerywa niedokończony start aplikacji (wołać przy spokojnej galerii); kolejność `list_assets` może różnić się od kolejności galerii w asercjach. Zmiana nieaddytywna, więc krótki ADR. Rekomendacja: zrobić.
- **Temat 6, audyty.** zod `^4.4.3` → 4.6.6 (zakres w `package.json` już na to pozwala; zmienia się `bun.lock`). `zip`: podbić do wersji z poprawkami (sprawdzić `cargo audit` w chwili zmiany) i ustawić `default-features = false, features = ["deflate"]`: backup zapisuje i czyta tylko `Deflated` (`backup_service.rs`), więc znikają `zstd`, `bzip2`, `aes`, `deflate64` i advisory `zstd*`. Koszt ~pół dnia; testy backup/restore Rust, `audit:js`, `audit:rust`. Ryzyko: zmiany API przy dużym skoku wersji `zip`; stare backupy są `Deflated`. ADR, jeśli `zip` zmieni wersję główną. Rekomendacja: zrobić (wymaga zgody na lockfile'y).
- **Temat 10, miesiące UTC czy lokalnie.**
  - A. Zostaje UTC (stan obecny, opisany w ADR 0007). Dla Polski skutek to assety z pierwszych 1–2 godzin miesiąca czasu lokalnego pod poprzednim miesiącem.
  - B. Opcjonalny `utcOffsetMinutes` w `start_asset_query` (stały offset z chwili zapytania), offset w kluczu sesji, sekcje liczone z `czas + offset`. Zmiana addytywna IPC, nowy ADR częściowo zastępujący 0007. Koszt ~0,5–1 dzień. Błąd DST ±1 h tylko dla dat z drugiej połowy roku przy granicy miesiąca.
  - C. Pełna strefa (np. `chrono-tz`): dokładne DST, ale nowa zależność.
  - Rekomendacja: **B**. Etykiety są widoczne dla użytkownika, koszt mały, nowa zależność niepotrzebna.
- **Temat 11, blokada zapisu ~2–3 s przy specu na 1M.** Data-safety wymaga „wszystko albo nic”, więc partie z commitami pośrednimi odpadają (albo potrzebny dziennik i rollback, czyli duży koszt).
  - A. Limit rozmiaru zapisu specu (np. 250k) z komunikatem. Proste, ale ogranicza funkcję.
  - B. Jedna transakcja jak dziś plus komunikat postępu w panelu bulk dla zapisów > 100k. Do tego zapisy w tle (skan, publikacja miniatur) ponawiają po `SQLITE_BUSY` zamiast zgłaszać błąd po 5 s busy timeout: dziś 2–3 s mieści się w 5 s, ale na wolnym dysku może nie.
  - Rekomendacja: **B**. Koszt ~1 dzień (Rust: ponawianie w ścieżkach w tle, test z `backend_integration`; TS: komunikat, 11 języków). Kanonizacja starej pisowni tagów zostaje na ścieżkach per asset; to osobna, opisana w ADR 0008 decyzja. ADR, bo dotyczy data safety.

### 2026-11-07 — orkiestrator (temat 9, druga próba)
- Zrobione: oba biegi `grid-baseline.e2e.js` na bezczynnym hoście (ok. 21:31–21:37), profil jak w „Przed” (XDG i `TMPDIR` pod `/var/tmp`, Xvfb 1440×1100). Wiersze (\*) w kolumnie „Po 08–09” etapu [07](07-verification-and-docs.md) zastąpione, warunki pomiaru zaktualizowane.
- Odkryte: obciążenie sprawdzane nie tylko przed startem. Monitor zapisywał co 10 s load i dwa procesy z największym CPU; gra nie pojawiła się w żadnej próbce. Szczyt load 16,8 to sam `media_tagger` przy `render_all_thumbnails` (85–86 s, fala D ~79 s). Obok przeglądarka ~0,8 rdzenia.
- Testy: `MEDIATAGGER_GRID_BASELINE=1 MEDIATAGGER_PERF=1 bun run test:e2e:tauri --spec e2e/specs/grid-baseline.e2e.js` → 3/3 w obu biegach. Strona IPC p50 3–7 / max 9–10 ms; serce do UI p50 24–25, p95 115–151, max 166–176 ms; like/tag 0/20; settle p50 16–17, skok paskiem p95 198–199 ms; klatka max 27–28 ms; DOM 407/98; cold per kafel p50 297–325, p95 405–426 ms, komplet widocznych p50 412–428 ms, kafle z danymi po 180–183 ms.
