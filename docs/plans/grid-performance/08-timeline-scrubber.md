# Etap 08: scrubber osi czasu

Fala E (po 05, bo korzysta z modelu scrolla, i po 03, bo korzysta z sort-key). Warstwa: Rust (sesja zapytania) i TypeScript (`features/gallery`). Zaakceptowany przez użytkownika.

Dokumentacja do przeczytania:
- `docs/subsystems/library-query-and-gallery.md`;
- `docs/subsystems/database.md` (porządek i buckety grup);
- `docs/architecture/ipc-contract.md`.

## Cel

Przy 1M elementów nikt nie „scrolluje” do 2015 roku. Pionowy scrubber z etykietami miesięcy i lat pozwala skoczyć w dowolne miejsce. Wzorem są Google Photos i Immich, z wnioskami z Immich #31360: ograniczyć rozmiar sekcji i nie liczyć wszystkiego w jednej blokującej pętli.

## Projekt

- **Sekcje liczone w Rust podczas budowy wektora ID sesji.** Porządek jest malejący po `sort_bucket_time` (etap 03), więc granice miesięcy wyznacza jedno przejście O(n) po już posortowanych wierszach. Potrzebna jest data bucketu dla każdego ID, czytana razem z ID.
  - Grupa mediów należy do miesiąca swojego bucketu. Granica sekcji nigdy nie rozcina grupy.
  - Brak daty trafia do osobnej sekcji „bez daty” na końcu, zgodnie z porządkiem.
  - Bardzo duży miesiąc dzielimy na kawałki (np. ≤ 5000 elementów) z tą samą etykietą. Scrubber pokazuje miesiąc raz, a precyzja skoku zostaje zachowana.
- **IPC.** Komenda `get_asset_query_sections(sessionId, revision)` zwraca kompaktową listę `{t: yyyymm, start, count}`, w JSON (~300–1000 wpisów) albo binarnie, jeśli wpisów jest dużo. Wynik `stale` przy zmianie rewizji. Pełna zmiana IPC.
- **Frontend.**
  - Komponent `GalleryTimelineScrubber` przy prawej krawędzi, powiązany z modelem scrolla z etapu 05.
  - Pozycja w scrubberze to `startIndex / total`. Grid ma stałą wysokość wierszy, więc mapowanie indeks → offset jest O(1): `rowTop(indexToRow(i))`.
  - Etykiety lat i miesięcy formatuje `Intl.DateTimeFormat` z językiem i18n.
  - Przeciąganie przesuwa grid w rAF. **Podczas przeciągania wstrzymany jest popyt** (strony, miniatury). Wznowienie następuje po puszczeniu albo po ~150 ms bezruchu.
  - Dostępność: `role="slider"` lub `scrollbar`, `aria-valuetext` z miesiącem, klawisze strzałek (miesiąc), PageUp/PageDown (rok), Home/End.
  - W trybie powyżej limitu wysokości scrubber może zastąpić własny pasek z etapu 05 albo z nim współistnieć. Decyzję i jej powód zapisz w notatkach.
- Sekcje nie dodają nagłówków w gridzie (bez zmiany wysokości wierszy). Nagłówki to osobny, przyszły temat, który wymagałby sum prefiksowych.

## Pliki

- `src-tauri/src/services/asset_query_service.rs`, `src-tauri/src/db/queries.rs`, `src-tauri/src/commands/assets.rs`, `src-tauri/src/lib.rs`
- `src/platform/tauri/*`, `contracts/*`, `src/library/query/*`
- `src/features/gallery/GalleryTimelineScrubber.tsx` (nowy), `GalleryGrid.tsx`, `createVirtualGrid.ts`
- `src/i18n/locales/*.json` (etykiety aria)
- `docs/subsystems/library-query-and-gallery.md`, `docs/architecture/ipc-contract.md`

## Testy

- Rust unit:
  - granice sekcji (zmiana miesiąca, grupa na granicy, brak daty, podział dużego miesiąca);
  - zgodność `start` i `count` z wektorem;
  - syntetyczne 1M w teście `#[ignore]` z czasem.
- Rust: komenda sekcji (stale, pusta biblioteka, filtr).
- Vitest: mapowanie pozycji na indeks i offset, klawiatura, aria, wstrzymanie popytu podczas przeciągania.
- E2E (orkiestrator): skok do najstarszego miesiąca daje widoczne kafle z tej sekcji.

## Kryteria akceptacji

- Skok w dowolny miesiąc przy 1M w < 300 ms do pierwszych danych.
- Brak blokującej pętli: budowa sekcji dodaje < 10% do czasu rebuildu wektora.

## Ryzyka

- Porządek, który nie jest czysto czasowy (buckety grup), może sprawić, że miesiąc pojawi się dwa razy. Algorytm musi to wykryć: scala sąsiednie sekcje z tą samą etykietą, a niesąsiednie dokumentuje.

## Status

todo — 2026-11-04

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->

### 2026-11-07 — agent etapu 05
- Z etapu 05: model scrolla jest w `src/features/gallery/createVirtualGrid.ts`, a czysta geometria w `gridGeometry.ts` ([ADR 0003](../../adr/0003-custom-gallery-virtualizer.md)).
  - Dla scrubbera: `scrollToIndex(index)` (wiersz indeksu na górę), `scrollTo(logical, userMotion)` (`userMotion` włącza pauzę popytu jak przy przeciąganiu), `maxOffset`, `windowed`, `range.visibleStartIndex`.
  - Offset jest logiczny, w px względem góry gridu.
- Z etapu 05: `offset` aktualizuje się co klatkę tylko w trybie okna, czyli przy treści powyżej `MAX_GRID_HEIGHT` = 16 000 000 px. Scrubber potrzebny też w trybie natywnym musi włączyć publikację `offset` zawsze, np. warunkiem w `publish`.
- Z etapu 05: powyżej limitu natywny pasek jest ukryty (`data-gallery-windowed`, `scrollbar-width: none` i oddany gutter jako `padding-right`). Własny `GalleryScrollbar.tsx` (`role="scrollbar"`, sticky przy prawej krawędzi kolumny galerii) pokazuje pozycję globalną.
  - Scrubber może go zastąpić. Wtedy zachowaj obsługę klawiatury (strzałki, PageUp/PageDown, Home/End) i `aria-valuenow`.
- Z etapu 05: limit 2^24 px (pomiar w `e2e/gallery-scale/height-probe.tsx`) dotyczy też pozycji, które scrubber wylicza dla sekcji. Pozycje fizyczne liczy tylko model okna (`rowTop - base`).
