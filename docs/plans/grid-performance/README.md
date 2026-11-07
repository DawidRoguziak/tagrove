# Wydajność gridu i niezawodność mutacji: plan etapowy

Status: **w realizacji**. Bieżący stan każdego etapu jest w jego sekcji `## Status` i w [NOTES.md](NOTES.md). Odnośniki `plik:linia` dotyczą commita `f71c8a9` i mogą się przesunąć; przed edycją agent sprawdza je przez `rg`.

## Cel

1. Płynny grid przy bibliotekach do 1 000 000 elementów (WebKitGTK, Linux).
2. Like (favorite) i dodawanie tagów działają zawsze, także podczas skanu i generowania miniatur.
3. Mniej kodu: usuwamy warstwy, które obchodzą biblioteki albo dublują pracę przeglądarki.

## Werdykty z analizy

- **Model bez stronicowania jest poprawny.** Backend trzyma uporządkowany wektor ID sesji (`Arc<Vec<i64>>`, `src-tauri/src/services/asset_query_service.rs`). Strona to wycinek tego wektora plus jedno `IN (...)`. Nie ma `OFFSET`, `COUNT(*)` ani zapytań N+1. `total` jest dokładny, a skok w dowolne miejsce kosztuje O(1) stron. Słabe punkty są gdzie indziej:
  - pełny reset przy `refresh()`;
  - zaznaczanie zakresu idzie przez cache widoku;
  - pozostałości sekwencyjnego ładowania (`hasMore`, `onReachEnd`);
  - rebuild wektora bez indeksu, wywoływany przez skan co 512 plików.
- **Własny virtualizer w Solid 2: tak (decyzja użytkownika).** TanStack virtual-core jest tu tylko kalkulatorem zakresu wierszy o stałej wysokości. Adapter woła prywatne `_didMount`/`_willUpdate`, ma własny licznik `version`, własną kompresję i obsługę wheel. Główne koszty to:
  - odmontowywanie wszystkich kafli przy szybkim scrollu;
  - przebudowa wszystkich komórek przy każdym evencie scrolla.

  Przy siatce kwadratów pozycja to czysta matematyka O(1).
- **Bug like/tagi:** zapisy na tym samym assecie są odrzucane zamiast kolejkowane. Pula 4 połączeń SQLite i lawina odświeżeń podczas skanu pogarszają problem. Szczegóły: etap 01 i 02.

## Porównanie z ogólną analizą „galeria 1M zdjęć”

| Pomysł | Decyzja | Gdzie |
| --- | --- | --- |
| Renderowanie tylko widocznych kafli, pula ~100 kafli w DOM, `contain`, rAF | przyjęte | 05 |
| Siatka kwadratów, pozycja O(1) | już jest, zostaje | 05 |
| Justified layout (Flickr, Google FlexLayout) | odrzucone: kwadraty wystarczą | — |
| Limit wysokości scrolla (WebKit ~33,5M px) | przyjęte: podnieść próg z 4M i dodać własny model scrolla powyżej limitu | 05 |
| Scrubber osi czasu z sekcjami miesięcy | przyjęte (decyzja użytkownika) | 08 |
| Keyset pagination, tabela podsumowań | nie dotyczy: wektor ID sesji daje O(1); koszt rebuildu rozwiązuje sort-key | 03 |
| Typed arrays zamiast miliona obiektów | częściowo: frontend trzyma najwyżej 12 stron; typed arrays tylko dla zakresów ID (binarne IPC) | 04, 09 |
| Fenwick, sumy prefiksowe | nie dotyczy przy stałej wysokości wierszy; sumy prefiksowe tylko dla sekcji scrubbera | 08 |
| Własny protokół `thumb://` zamiast batch IPC | przyjęte (decyzja użytkownika), zaczyna się od spike'u | 06 |
| Binarne IPC (`tauri::ipc::Response`) dla dużych danych | przyjęte dla zakresów ID | 04 |
| Skalowanie JPEG przy IDCT, szybsze ffmpeg | przyjęte; nowa zależność tylko za zgodą | 06 |
| Miniatury 2× dla HiDPI | sprawdzić i ewentualnie dodać drugi rozmiar | 06 |
| ThumbHash | odrzucone (decyzja użytkownika) | — |
| Zaznaczenie jako spec rozwiązywany w Rust | przyjęte (decyzja użytkownika) | 09 |
| WebGL z atlasem, powierzchnia wgpu | odrzucone: ryzyko wolnej ścieżki WebKitGTK, trzeba by samemu robić hit-testing i a11y | — |
| Testy WebView2 i WKWebView | nie dotyczy: aplikacja tylko na Linuksa | — |
| LRU zdekodowanych bitmap w bajtach | nie dotyczy: przy `<img>` pamięcią zarządza WebKit | — |
| Miniatury jako bloby w SQLite | odłożone: po wprowadzeniu `thumb://` to szczegół wewnętrzny; najpierw pomiar, zmiana dotyka też backupu | — |

## Co wycinamy z kodu

- `@tanstack/virtual-core` i hacki adaptera w `src/features/gallery/createVirtualGrid.ts`.
- Mapowanie kompresji z przechwytywaniem wheel (`src/features/gallery/scrollCoordinates.ts`).
- Po etapie 06:
  - galeryjna kolejka miniatur (`src/library/thumbnails/createThumbnailQueue.ts`, `thumbnailStore.ts`, `createAssetThumbnail.ts`);
  - galeryjna ścieżka `ensure_thumbnails`;
  - pole `thumb_path` w `AssetSummary`.
- Martwa komenda `list_assets` (`COUNT` + `OFFSET`).
- Pozostałości `hasMore`, `onReachEnd`, `handleReachEnd`.
- Zduplikowane indeksy SQLite.
- Osobne guardy favorite i tagów w lightboxie oraz bulk (zastępuje je jedna kolejka per asset).

## Etapy

| Etap | Plik | Warstwa | Fala |
| --- | --- | --- | --- |
| 01 | [01-mutation-queue.md](01-mutation-queue.md) | TS: `library/metadata`, `features/lightbox`, `features/bulk` | A |
| 02 | [02-backend-contention.md](02-backend-contention.md) | Rust: skan, pula, blokady, zapisy miniatur | A |
| 05 | [05-custom-virtual-grid.md](05-custom-virtual-grid.md) | TS: `features/gallery`, `ui/controls/ThumbnailImage.tsx` | A |
| 03 | [03-sort-key-and-indexes.md](03-sort-key-and-indexes.md) | Rust: `db/schema.rs`, `db/queries.rs` | B |
| 04 | [04-query-cache.md](04-query-cache.md) | TS `library/query` + nowa komenda IPC | B |
| 06 | [06-thumb-protocol.md](06-thumb-protocol.md) | Rust + TS, cały stos miniatur | C |
| 07 | [07-verification-and-docs.md](07-verification-and-docs.md) | orkiestrator: pomiary, E2E, dokumentacja | D |
| 08 | [08-timeline-scrubber.md](08-timeline-scrubber.md) | Rust sesja + TS gallery | E |
| 09 | [09-selection-spec.md](09-selection-spec.md) | Rust bulk + TS selection/bulk | E |
| 10 | [10-open-topics.md](10-open-topics.md) | orkiestrator: otwarte tematy po fali E (część A bez decyzji, część B za zgodą) | F |

### Fale i równoległość

Worktree są zabronione (`CLAUDE.md`). Równolegle pracują tylko agenci na **rozłącznych plikach**:

- **Fala A:** 01 ∥ 02 ∥ 05.
- **Fala B:** 03 ∥ 04. Etap 04 zmienia `start_asset_query` i dodaje komendę w `commands/assets.rs`; etap 03 zmienia `db/queries.rs` i `db/schema.rs`. Plik `src-tauri/src/lib.rs` (`generate_handler!`) edytuje tylko jeden agent naraz: 03 usuwa `list_assets`, 04 dodaje komendę. Kolejność: najpierw 03, potem 04.
- **Fala C:** 06, po 05, bo dotyka `GalleryTile` i `ThumbnailImage`.
- **Fala D:** 07, pomiary po falach A–C.
- **Fala E:** 08 ∥ 09. Oba zmieniają `asset_query_service.rs`, więc część Rust robi jeden agent albo robią ją sekwencyjnie.
- **Fala F:** 10, po weryfikacji fali E. Każdy temat części B to osobny commit po decyzji użytkownika.

Etap 07 ma też krok 0 (pomiary bazowe), który orkiestrator wykonuje **przed** falą A.

## Zasady dla agentów

- Przed startem przeczytaj ten README, swój etap, [NOTES.md](NOTES.md) oraz strony dokumentacji wskazane w etapie.
- **Desktop E2E uruchamia tylko orkiestrator, jeden test naraz.** Wspólne `dist/`, `src-tauri/target-e2e` i porty. Agenci uruchamiają:
  - wybrane pliki Vitest, nie cały `bun run test` (8 GiB heap, równoległe agenty mogą zabraknąć pamięci);
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked <filtr>`.
- **Zmiana IPC w jednej zmianie:** komenda i model w Rust, wrapper `src/platform/tauri`, typ w `contracts/`, lista `generate_handler!`, `docs/architecture/ipc-contract.md`. Klucze argumentów w camelCase, pola odpowiedzi w snake_case.
- **Schemat** zmienia się tylko przez idempotentne `init_schema`. Nie ma migration runnera.
- **Zapis**, który zmienia wyniki zapytań, bumpuje rewizję biblioteki w tej samej transakcji.
- **Kolejność blokad:** maintenance gate → `scan_lock` → `thumb_lock`, tylko przez helpery z `app/locks.rs`. Nigdy nie trzymaj ich przez `.await`.
- **Zmiana tekstu UI lub aria** wymaga aktualizacji selektorów E2E i recept `verify-mediatagger`.
- **Zależności:** `cargo` zawsze z `--locked`. Nowa zależność (crate albo npm) tylko za zgodą użytkownika i jako jawna zmiana lockfile'a.
- **Repozytorium jest publiczne:** w notatkach i fixture'ach żadnych prywatnych ścieżek, nazw plików użytkownika ani logów z prywatnymi danymi.
- **Źródło wygrywa z dokumentacją.** Zmiana zachowania aktualizuje stronę dokumentacji właściwą dla podsystemu w tej samej zmianie.
- **Commit** robisz dopiero po akceptacji użytkownika, osobno dla każdego etapu.

## Protokół notatek

Każdy plik etapu kończy się sekcjami `## Status` i `## Notatki agentów`.

- `## Status`: jedna z wartości `todo`, `in progress`, `blocked`, `done`, `done (plan B)`, plus data.
- `## Notatki agentów`: wpisy dopisywane na końcu, nigdy nadpisywane:

  ```
  ### YYYY-MM-DD — <rola, np. agent etapu 02>
  - Zrobione: …
  - Odkryte: … (plik:linia)
  - Zostało: …
  - Testy: `<komenda>` → pass/fail (krótko, bez prywatnych danych)
  ```

- Po każdym wpisie dopisz jedną linię do [NOTES.md](NOTES.md). Wtedy kolejny agent lub orkiestrator widzi przekazanie bez czytania wszystkich etapów.
- Jeśli odkrycie zmienia inny etap, dopisz notatkę **w tamtym etapie** z prefiksem `Z etapu NN:`.
