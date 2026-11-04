# Etap 07: pomiary, weryfikacja end-to-end i zgodność dokumentacji

Wykonuje orkiestrator. Krok 0 jest **przed** falą A, reszta w fali D (po etapach 01–06), a ponowna weryfikacja po fali E.

Dokumentacja do przeczytania:
- `docs/development/testing.md` (wybór warstwy testu, izolacja E2E, flagi `MEDIATAGGER_*`);
- skill `verify-mediatagger`.

## Krok 0: pomiary bazowe (przed zmianami)

Wszystko na **syntetycznych** danych w katalogach tymczasowych. Nigdy nie używaj prywatnych folderów ani prawdziwych danych aplikacji.

1. **Generator fixture'ów.**
   - Skrypt (np. w `scripts/` albo jako test diagnostyczny `#[ignore]`) tworzy bazę 100k i 1M wierszy w `tempfile`, z grupami, tagami i favorites.
   - Do E2E: folder tymczasowy z ~5–20k małych wygenerowanych JPEG i kilkoma krótkimi wideo (ffmpeg `testsrc`).
2. **Backend** (`MEDIATAGGER_PERF=1`):
   - czas `start_asset_query` (rebuild wektora) dla 100k i 1M, bez filtrów i z filtrami tag i favorite;
   - czas `get_asset_query_page`;
   - czas zapisu favorite i tagu bez skanu i podczas skanu;
   - liczba rebuildów podczas skanu.
3. **Frontend** (E2E na Xvfb, `verify-mediatagger`):
   - czas od zatrzymania flingu do kompletu kafli;
   - najdłuższa klatka po zatrzymaniu (`PerformanceObserver` `longtask`, jeśli dostępny w WebKitGTK; w przeciwnym razie pętla rAF);
   - liczba węzłów DOM w gridzie;
   - czas od wejścia kafla w viewport do pojawienia się miniatury przy pustym cache.
4. **Scenariusz błędu like i tagów:** skan 20k plików, w trakcie 20 kliknięć serca i dodań tagu. Licznik nieudanych i opóźnionych operacji.
5. Wyniki wpisz do sekcji „Wyniki” poniżej: liczby, bez ścieżek i nazw prywatnych plików.

## Fala D: weryfikacja po etapach 01–06

1. Powtórz pomiary z kroku 0 i uzupełnij tabelę.
2. Pełne bramki jakości:
   - `bun run quality`;
   - `bun run test` (jeśli zabraknie pamięci: `-- --maxWorkers=1`);
   - `bun run test:backend`;
   - `bun run privacy:check && bun run test:privacy`;
   - `git diff --check`.
3. Desktop E2E, **jeden po drugim**:
   - `e2e/specs/app.smoke.e2e.js`;
   - specy galerii, lightboxa i bulk (pełna lista w `docs/development/testing.md`, „Real desktop E2E”, z wymaganymi flagami `MEDIATAGGER_<FLAG>=1`).
4. `verify-mediatagger`:
   - fling i PageDown;
   - Ctrl+wheel zoom;
   - zaznaczanie prostokątem;
   - Shift-click przez duży zakres;
   - like i tag w lightboxie oraz w bulk podczas skanu;
   - delete i rename podczas generowania miniatur.

   Zrzuty ekranu tylko z danych syntetycznych.
5. **Zgodność dokumentacji.** Każda strona zmieniona przez etapy opisuje aktualne zachowanie, a odnośniki i komendy działają:
   - `docs/subsystems/library-query-and-gallery.md`, `thumbnails.md`, `database.md`, `search-tags-and-media-groups.md`, `scanning-and-indexing.md`, `lightbox.md`, `data-safety-and-portability.md`;
   - `docs/architecture/ipc-contract.md`, `system-overview.md`;
   - `docs/frontend/architecture-and-ui-conventions.md`;
   - `docs/development/testing.md`;
   - `.claude/rules/*.md`, `CLAUDE.md` (Stack: brak TanStack virtual-core).
6. Zaktualizuj statusy etapów i NOTES.md. Zbierz listę tego, co pozostało niezweryfikowane.

## Wyniki

| Metryka | Przed | Po 01–06 | Po 08–09 |
| --- | --- | --- | --- |
| Rebuild wektora 100k / 1M (ms) | | | |
| Strona galerii (ms) | | | |
| Favorite bez skanu / podczas skanu (ms) | | | |
| Nieudane operacje like/tag w scenariuszu (n/20) | | | |
| Rebuildy podczas skanu 20k | | | |
| Stop flingu → komplet kafli (ms) | | | |
| Najdłuższa klatka po flingu (ms) | | | |
| Węzły DOM w gridzie | | | |
| Viewport → miniatura, pusty cache (ms) | | | |
| Miniatura JPEG 24 MP (ms) | | | |

## Status

todo — 2026-11-04

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->
