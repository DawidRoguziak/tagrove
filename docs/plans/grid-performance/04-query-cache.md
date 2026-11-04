# Etap 04: page cache, refresh i zakresy ID

Fala B, po etapie 03 w części dotyczącej `lib.rs`. Warstwa: TypeScript `src/library/query` plus dwie zmiany IPC.

Dokumentacja do przeczytania:
- `docs/subsystems/library-query-and-gallery.md`;
- `docs/architecture/ipc-contract.md` (query sessions);
- reguła `.claude/rules/gallery-query.md`.

## Problemy (potwierdzone w kodzie)

1. **`refresh()` wyrzuca wszystko.** `src/library/query/createAssetQuery.ts:174` woła `resetThumbnailQueue()`, czyli czyści store miniatur. Cache startuje od strony 0 (`:213`). Refreshe przychodzą:
   - ze skanu;
   - z importu CSV;
   - z mutacji wymagających odświeżenia;
   - z wyników `stale` (`:204-206, 264-266, 296`).

   Głęboko w liście wszystkie kafle robią się puste, a potrzebne są 2 kolejne IPC (`start`, potem `page`).
2. **`mergePage` kopiuje cały cache** (dwie `Map` do ~1536 wpisów, przebudowa tablicy `assets`) przy każdej stronie (`createAssetQuery.ts:132-133`). To blokuje podniesienie limitu 12 stron.
3. **Shift-click i select-all** (`src/features/gallery/createSelection.ts:58`, `getIdsRangeAsync` w `createAssetQuery.ts:360-385`):
   - zakres jest dzielony na kawałki po 128;
   - każdy kawałek to sekwencyjne `loadPage`, który wrzuca stronę do LRU i robi pełny publish;
   - przy 100k to ~780 IPC i sekundy pracy.
4. **Pozostałości ładowania sekwencyjnego.** `hasMore = offset < total`, `handleReachEnd` i `onReachEnd` (`src/app/AppGalleryView.tsx`, `createVirtualGrid.ts`). Po skoku na koniec `hasMore` daje fałsz.
5. **Brak debounce** dla refreshy wywołanych przez `stale`. Podczas skanu jeden refresh goni drugi. Etap 02 ogranicza bumpy, a ten etap zabezpiecza frontend.

## Kroki

1. **Refresh z kotwicą.**
   - Nowy opcjonalny argument `anchorOffset` w `start_asset_query`. Rust zwraca w odpowiedzi stronę zawierającą ten offset zamiast strony 0. Pełna zmiana IPC: Rust, wrapper, `contracts/`, `ipc-contract.md`.
   - Frontend przekazuje `visibleStartIndex` z bieżącego viewportu.
   - Do czasu etapu 06 `refresh()` **nie czyści** store miniatur; ścieżki trzymane per asset ID zostają, a `thumb_path` z nowej strony je potwierdzi. Po etapie 06 store znika.
2. **Debounce refreshy `stale`** (np. 250 ms trailing, maksymalnie 1 na ~1 s podczas skanu). Refresh wywołany przez użytkownika lub mutację działa od razu.
3. **Mutowalny page cache.** Mapy mutowane w miejscu plus sygnał `version` (Solid 2: sygnał licznika, czytany przez `getAssetAt`). Bez kopiowania całych map. Po zmianie rozważ limit 24–32 stron (~40 KB JSON na stronę); zaktualizuj `.claude/rules/gallery-query.md` i `library-query-and-gallery.md`.
4. **Komenda zakresu ID (binarna).**
   - Nowa komenda, np. `get_asset_query_id_range(sessionId, revision, start, end)`. Zwraca `tauri::ipc::Response` z surowymi bajtami i64 LE z wektora sesji, bez SQL.
   - Frontend czyta przez `BigInt64Array` albo `Float64Array`. ID są < 2^53; sprawdź i udokumentuj wybór.
   - Odpowiedź `stale` przy zmianie rewizji. **Semantyka bez zmian:** `getIdsRangeAsync` odrzuca cały zakres, nigdy nie obcina.
   - `createSelection` używa nowej komendy jednym wywołaniem (ewentualnie chunkami po ~1M) bez dotykania page cache.
   - Pełna zmiana IPC i test Rust.
5. **Usuń `hasMore`, `onReachEnd` i `handleReachEnd`** wraz z propsami (`AppGalleryView.tsx`, `GalleryGrid.tsx`, `GalleryStatusFooter.tsx`). Uwaga na konflikt z etapem 05: jeśli etap 05 jest jeszcze w toku, zostaw notatkę w `05-custom-virtual-grid.md` i zrób to po nim.

## Pliki

- `src/library/query/createAssetQuery.ts`, `src/library/createLibrary.ts`
- `src/features/gallery/createSelection.ts` (tylko użycie nowej komendy)
- `src/app/AppGalleryView.tsx`
- `src/platform/tauri/*` i `contracts/*` (query)
- `src-tauri/src/commands/assets.rs`, `src-tauri/src/services/asset_query_service.rs`, `src-tauri/src/lib.rs`
- `docs/architecture/ipc-contract.md`, `docs/subsystems/library-query-and-gallery.md`, `.claude/rules/gallery-query.md`

## Testy

- `src/library/query/__tests__/createAssetQuery.test.ts`:
  - refresh z kotwicą ładuje stronę viewportu;
  - refresh nie czyści miniatur;
  - debounce `stale`;
  - mutowalny cache i LRU.
- `src/library/query/__tests__/queryRaces.test.ts`: wyścigi generacji z nowym refreshem.
- `src/features/gallery/__tests__/createSelection.test.ts`: Shift-click na 100k to 1 wywołanie; stale odrzuca cały zakres.
- Test API frontendu dla nowej komendy (dekodowanie bajtów).
- Rust: test komendy zakresu (granice, stale, pusty zakres) i test `anchorOffset`.
- `bun run test -- src/library/query src/features/gallery/__tests__/createSelection.test.ts`, `cargo test --locked <filtr>`.

## Kryteria akceptacji

- Refresh głęboko w liście nie pokazuje pustych kafli. Pierwsza widoczna strona przychodzi w jednym IPC.
- Shift-click przez 100k elementów kończy się w < 200 ms.
- Brak `hasMore` i `onReachEnd` w kodzie.

## Ryzyka

- Mutowalny cache i reaktywność Solid 2 RC: pamiętaj, że odczyt w `getAssetAt` musi śledzić `version`. Testy z Testing Library muszą to pokryć.
- Binarny `Response` w testach z mockiem `invoke`: udokumentuj w `docs/development/testing.md`, jak go mockować.

## Status

todo — 2026-11-04

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->
