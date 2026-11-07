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

done — 2026-11-07 (czeka na akceptację i commit)

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

### 2026-11-07 — agent etapu 08
- Zrobione:
  - `SectionBuilder` w `src-tauri/src/db/query_sections.rs`. Ścieżka z filtrami liczy sekcje w tym samym przejściu co wektor ID, ze statystyk bucketów (`OrderedAssetIds { ids, sections: Some(..) }`). Ścieżka bez filtrów czyta tylko ID; jej sekcje czyta `list_unfiltered_query_sections` (sam `sort_bucket_time`, ten sam indeks i porządek) przy pierwszym `get_asset_query_sections`, w transakcji z kontrolą rewizji, i zapisuje w sesji (`OnceLock`).
  - Sesja trzyma `sections`; nowa komenda `get_asset_query_sections(sessionId)` → `ready { session_id, total, sections }` / `stale` (JSON). Pełna zmiana IPC: Rust, `models.rs`, `generate_handler!`, wrapper i `contracts/`, `api.test.ts`, `ipc-contract.md`. [ADR 0007](../../adr/0007-session-timeline-sections.md).
  - Frontend: `createAssetQuery.sections` (jedno wczytanie na sesję; trafienie w cache z tym samym ID nie czyta ponownie), `timeline.ts` (scalanie podzielonych miesięcy, `monthAt`, `index/total`, klawisze, `Intl.DateTimeFormat` w UTC), `GalleryTimelineScrubber.tsx` (`role="slider"` „Timeline”, `aria-valuetext` z miesiącem), `holdDemand`/`resumeDemand` w `createVirtualGrid.ts`, klucze `gallery.timelineLabel`/`timelineNoDate` w 11 językach.
- Decyzje (szczegóły w ADR 0007):
  - Etykieta to miesiąc **czasu bucketu**, nie `modified_at` assetu. Porządek jest malejący po czasie bucketu, więc miesiąc nigdy nie rośnie wzdłuż wektora: grupa nigdy nie jest rozcięta, miesiąc nie może wystąpić dwa razy (ryzyko z sekcji „Ryzyka” znika z konstrukcji), a sąsiednie sekcje z tą samą etykietą to tylko celowy podział > 5000.
  - Miesiące w UTC (brak bazy stref w backendzie, sesje współdzielone po kluczu). „Bez daty” = czas ≤ 0 (indexer zapisuje 0, gdy nie ma czasu pliku), zawsze na końcu.
  - Scrubber **współistnieje** z paskiem pozycji, nie zastępuje `GalleryScrollbar`: pasek jest jedyną kontrolką pozycji dla wyników z jednym miesiącem albo bez dat i ma inne kroki klawiatury (wiersz/strona). Szyna stoi 16 px od prawej krawędzi w obu trybach.
  - Skok idzie przez `scrollToIndex` (wiersz indeksu na górę, O(1)). Przy końcu listy scroll się zatrzymuje, więc pierwszy widoczny element może należeć do wcześniejszego miesiąca; scrubber pamięta wtedy własny cel (kursor) do następnego ruchu widoku z innego powodu.
- Odkryte:
  - **Regresja znaleziona w weryfikacji po fali E i poprawiona:** pierwsza wersja czytała `sort_bucket_time` i `sort_bucket_id` dla każdego wiersza w rebuildzie bez filtrów. Sam builder kosztuje 1,5 ms na 1M, ale odczyt dwóch kolumn więcej podwoił strumień SQL (29 → 58 ms) i rebuild 1M (32 → 67–69 ms, trzy biegi release), czyli ~+100% zamiast < 10%. Teraz sekcje bez filtrów są leniwe: rebuild 1M 33–34 ms (100k 3,9 ms), pierwszy odczyt sekcji 28,5–28,8 ms (100k 2,7 ms) poza ścieżką pierwszej strony, kolejne 0,02 ms. Kryterium „< 10% rebuildu” jest spełnione dla rebuildu; koszt sekcji przeniósł się na asynchroniczny odczyt po `ready`.
  - Liczenie miesiąca per wiersz kosztowało 7,75 ms na 1M (~24% rebuildu 32 ms). Po buforowaniu granic bieżącego miesiąca: 1,5 ms (~5%), test `one_million_rows_build_quickly` (`#[ignore]`, release).
  - Nakładkowy pasek przewijania GTK nie ma guttera i przechwytuje trafienia w prawych ~14 px klienta. Pierwsza wersja szyny (4 px od krawędzi) była nieklikalna także dla prawdziwej myszy. Teraz 16 px od krawędzi.
  - WebKitWebDriver odrzuca `.click()` na szynie („not interactable”, kontener sticky ma `pointer-events: none`), choć natywne akcje wskaźnika do niej docierają. Spec skupia ją przez `focus()` i używa natywnego przeciągania.
  - Solid renderuje `data-scroll-paused={true}` jako pusty atrybut, a `false` usuwa atrybut; tak samo czytają go `grid-baseline` i `gallery-scale`.
- Zostało: nagłówki sekcji w gridzie (osobny temat, sumy prefiksowe); strefa czasowa lokalna wymagałaby offsetu w kluczu sesji.
- Testy:
  - `cargo test --locked --lib query_sections sections_follow try_list_ordered` → pass; `--release --lib one_million_rows_build_quickly -- --ignored` → 1,54 ms / 1M, 229 sekcji.
  - Vitest: `timeline.test.ts`, `GalleryTimelineScrubber.test.tsx`, `createVirtualGrid.test.tsx` (demand hold, `offsetOfIndex`), `GalleryGrid.test.tsx` (scrubber obok paska trybu okna, skok do najstarszego miesiąca przy 1M), `createAssetQuery.test.ts` (sekcje raz na sesję), `api.test.ts` → pass.
  - E2E `MEDIATAGGER_GALLERY_TIMELINE=1 … gallery-timeline.e2e.js` (12 005 PNG, 13 miesięcy, prywatny Xvfb, XDG pod `/tmp/mediatagger-*`) → 3/3 w piątym biegu. Biegi 1–4 padły na: szynę pod paskiem GTK (błąd aplikacji, poprawiony), `.click()` WebDrivera, odczyt `data-scroll-paused`, kursor przy końcu listy (błąd aplikacji, poprawiony) i usunięte w WDIO 9 `isDisplayedInViewport`. Klawisz → dane kafla celu: End 88 ms, ArrowUp 68, Home 125, PageDown 76 (debug, 12k).
