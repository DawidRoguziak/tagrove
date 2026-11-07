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

done — 2026-11-07 (faza A w commicie bf0afd9; faza B, kroki 1-IPC i 4: [ADR 0005](../../adr/0005-binary-id-ranges-and-anchored-refresh.md))

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->

### 2026-11-07 — agent etapu 05
- Z etapu 05: etap 05 zostawił interfejs `hasMore`/`isLoading`/`onReachEnd` w `createVirtualGrid` bez zmian (efekt reach-end na końcu pliku) i `GalleryStatusFooter` nietknięty. Można je usuwać zgodnie z krokiem 5.
  - Wywołania w harnessie: `e2e/gallery-scale/fixture.tsx` (licznik `reachEndCalls`) i `performance.tsx` (`onReachEnd={model.handleGalleryReachEnd}`).
- Z etapu 05: `getAssetAt(index)` jest wołane z memo każdej komórki puli wierszy (`GalleryGrid.tsx`, `assetAt`). Mutowalny cache z sygnałem `version` musi więc dalej powiadamiać te memo. Pokrywa to test „keeps loaded tiles mounted through a fast jump…” w `GalleryGrid.test.tsx`.

### 2026-11-07 — agent etapu 03
- Z etapu 03: etap 03 nie zmienia `src-tauri/src/lib.rs`, `commands/assets.rs`, `src/platform/tauri/*` ani `docs/architecture/ipc-contract.md`. `list_assets` zostaje (usunięcie odłożone, patrz etap 07), więc te pliki są wolne dla etapu 04.
- Z etapu 03: rebuild wektora ID przy 1M trwa teraz ~32 ms bez filtrów i 36–420 ms z filtrami (release). Wektor z filtrami jest budowany w Rust (`try_list_ordered_asset_ids_with_meta`, `db/queries.rs`), z kontrolą anulowania przed sortowaniem i po nim. Pojemność wektora sesji jest dokładna (nowa alokacja), co jest ważne dla limitu pamięci sesji.

### 2026-11-07 — agent etapu 04 (faza A)
- Zrobione (tylko TS, bez IPC):
  - cache stron mutowany w miejscu, jeden sygnał `version` (czytają go `getAssetAt` i memo `assets`), `MAX_CACHED_PAGES` 12 → 24 (`src/library/query/createAssetQuery.ts`);
  - `refresh()` nie woła już `resetThumbnailQueue()`; opcja zniknęła z `createAssetQuery`. Stare strony bieżącego zakresu zostają jako fallback tylko dla `getAssetAt` (decyzja użytkownika); `onAssetsChanged` dostaje też wiersze fallbacku, więc `thumbnailStore.retain` nie gubi ich ścieżek;
  - debounce `stale`: jeden refresh po 250 ms, kolejne `stale` dołączają, min. 1 s odstępu; jawny refresh kasuje timer;
  - usunięte `hasMore`, `isLoading` (virtualizer), `onReachEnd`, `handleReachEnd`, `handleGalleryReachEnd` i getter `offset`; stopka pokazuje koniec wyników zawsze, gdy miniatury się nie generują.
- Odkryte:
  - przy 24 stronach `selection-recovery.e2e.js` (2052 pliki, ~16 stron) przestaje wymuszać ewikcję strony 128; spec przepinam w fazie B na komendę zakresu ID (zgoda użytkownika);
  - `gallery.performance.e2e.js` dalej przechodzi, ale też już nie wymusza ewikcji.
- Zostało (faza B): `anchorOffset` w `start_asset_query`, binarna komenda zakresu ID (f64 LE, decyzja użytkownika) w `createSelection`, ADR 0005, desktop E2E, pomiar Shift-click 100k.
- Testy: `bun run test -- src/library src/features/gallery src/app src/features/lightbox/__tests__/createLightbox.test.ts src/features/bulk/__tests__/createBulk.test.ts` → pass (372); `bun run typecheck`, `bun run lint`, `tsc -p e2e/gallery-scale/tsconfig.json` → pass. Desktop E2E jeszcze nie.

### 2026-11-07 — agent etapu 04 (faza B)
- Zrobione:
  - `start_asset_query` ma opcjonalny `anchorOffset`. `AssetQueryManager::start_at` zwraca wyrównaną stronę z kotwicą (obciętą do ostatniego wiersza) i jej `offset`, zarówno przy nowej sesji, jak i przy trafieniu w cache. `start` bez zmian sygnatury deleguje do `start_at(None)`, więc wywołania w `backup_service.rs` zostały nietknięte. `createLibrary` podaje `visibleStartIndex`.
  - Nowa komenda `get_asset_query_id_range(sessionId, start, end)` zwraca `tauri::ipc::Response`: f64 LE `[count, ...ids]`, pusty bufor = `stale`, id spoza `0..=2^53` → błąd. Frontend dekoduje ją przez `decodeAssetIdRange` (`Float64Array`).
  - `getIdsRangeAsync` czyta zakresy tylko tą komendą, w chunkach ≤ 1M, bez stron i bez LRU. `createSelection` robi jedno wywołanie na kopertę zakresów.
  - Rozważony i odrzucony argument `revision`: o nieaktualności decyduje `dependency_revision` sesji, tak jak w `page`/`position` (ADR 0005).
  - `e2e/specs/selection-recovery.e2e.js` wstrzykuje błąd w `get_asset_query_id_range` (`start: -1`) i sprawdza dokładnie jeden udany odczyt `0..=endpoint` po Retry.
  - Helpery testowe w `src/test/idRange.ts`; mockowanie opisane w `docs/development/testing.md`.
- Pomiary (maszyna bez obciążenia drugiej sesji: brak `cargo`/`vitest`/`wdio`, load ~1; w drzewie tylko zmiany fazy B etapu 04 na `bf0afd9`):
  - Rust release (`grid_baseline query_rebuild_and_page_baseline`, nowa metryka `get_asset_query_id_range`): zakres 100k 0,05 ms (800 KB), 1M 0,45 ms (8 MB).
  - Desktop (build E2E debug, Xvfb, 100k kopii PNG, tymczasowy spec poza repo): surowe IPC zakresu 100k 3–4 ms (`ArrayBuffer`); **Shift-click 0 → 99 999 od kliknięcia do „Selected: 100000”: 15–17 ms** (5 prób, dwa biegi). Przed zmianą ~780 sekwencyjnych IPC stron.
  - Desktop, refresh na indeksie 50 000 (Search bez zmian filtrów): jedno IPC `start_asset_query` z `anchorOffset: 50000` → `offset: 49920`, zero `get_asset_query_page`, 0 placeholderów w widoku przez 240 klatek.
  - Vitest/jsdom: rozwiązanie selekcji 100k z dekodowaniem bufora 3–6 ms.
- Testy:
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked` → pass (lib 219, backend_integration 11, pozostałe cele); `bun run rust:fmt`, `bun run rust:clippy` → pass;
  - `bun run test -- src/library src/features/gallery src/features/bulk src/features/lightbox/__tests__/createLightbox.test.ts src/app src/platform` → pass (438); `bun run typecheck`, `bun run lint` → pass;
  - desktop E2E (XDG pod `/tmp/mediatagger-stage04-*`, `TMPDIR=/var/tmp`, profil usuwany po biegu): `gallery-browsing` 2/2, `gallery-selection` 7/7, `selection-recovery` 2/2, `app.smoke` 4/4;
  - `grid-baseline` (`MEDIATAGGER_GRID_BASELINE=1 MEDIATAGGER_PERF=1`) 3/3: skan 20k p50 691 ms, like/tag 0/20 nieudanych, 1 start i 1 strona `stale` podczas skanów, settle p50 17 ms, 211 węzłów; bez regresji względem biegu etapu 03.
