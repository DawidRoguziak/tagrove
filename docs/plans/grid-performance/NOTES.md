# Dziennik przekazań

Jedna linia na wpis, najnowsze na dole. Szczegóły trafiają do sekcji `## Notatki agentów` w pliku etapu.

Format: `YYYY-MM-DD | etap NN | rola | status | jedno zdanie | link do etapu`

---

2026-11-04 | — | planowanie | plan | Utworzono plan etapowy na podstawie analizy frontendu, backendu i ścieżki mutacji | [README](README.md)
2026-11-06 | etap 07 | orkiestrator | in progress | Krok 0: pomiary bazowe (`grid_baseline.rs`, `grid-baseline.e2e.js`), kolumna „Przed” wypełniona | [07](07-verification-and-docs.md)
2026-11-06 | etap 01 | orkiestrator | todo | Z etapu 07: błąd like odtworzony, 9–15/20 kliknięć serca odrzuconych we frontendzie bez IPC; backend 0 błędów | [01](01-mutation-queue.md)
2026-11-06 | etap 02 | orkiestrator | todo | Z etapu 07: zapisy podczas skanu max ~0,3–0,7 s, bez błędów i bez czekania na pulę; ~45 bumpów rewizji na skan 20k | [02](02-backend-contention.md)
2026-11-06 | etap 03 | orkiestrator | todo | Z etapu 07: rebuild 30 ms (100k) / 377–383 ms (1M), wykluczenie tagu 657–665 ms; plan z dwoma temp B-tree | [03](03-sort-key-and-indexes.md)
2026-11-06 | etap 05 | orkiestrator | todo | Z etapu 07: baza gridu (stop → komplet 16–193 ms, klatki 17–25 ms, 379–589 węzłów) | [05](05-custom-virtual-grid.md)
2026-11-06 | etap 06 | orkiestrator | todo | Z etapu 07: miniatura 24 MP 99 ms (release); pusty cache p50 354 ms na kafel (debug) | [06](06-thumb-protocol.md)
2026-11-06 | etap 01 | agent etapu 01 | done | Kolejka FIFO zapisów per asset ([ADR 0001](../../adr/0001-per-asset-metadata-write-queue.md)); w `grid-baseline` like 0/20 i tag 0/20 nieudanych | [01](01-mutation-queue.md)
2026-11-06 | etap 02 | agent etapu 01 | todo | Z etapu 01: zapisy pod skanem do ~3 s (debug) widać teraz jako czas pending serca | [02](02-backend-contention.md)
2026-11-06 | etap 07 | agent etapu 01 | in progress | Z etapu 01: bieg pośredni `grid-baseline` (like/tag 0/20); kolumna „Po 01–06” czeka na fale A–C | [07](07-verification-and-docs.md)
2026-11-07 | etap 02 | agent etapu 02 | done | Priorytet zapisów użytkownika, pula 8 z rezerwą, ≤ 1 bump/s w skanie, `thumb_lock` per sekcja ([ADR 0002](../../adr/0002-backend-write-priority-and-scan-revisions.md)); E2E: like/tag 0/20, tag max 33 ms, 0 rebuildów pod skanem | [02](02-backend-contention.md)
2026-11-07 | etap 06 | agent etapu 02 | todo | Z etapu 02: guard miniatur per sekcja, `commit_thumbnail_results`, istniejąca miniatura bez zapisu | [06](06-thumb-protocol.md)
2026-11-07 | etap 07 | agent etapu 02 | in progress | Z etapu 02: pomiary pośrednie Rust i E2E; do sprawdzenia `.claude/rules` (thumbnails, database, backend-core) | [07](07-verification-and-docs.md)
2026-11-07 | etap 05 | agent etapu 05 | done | Własny virtualizer z pulą wierszy i natywnym oknem scrolla (`MAX_GRID_HEIGHT` 16M z bramki 2^24 px), virtual-core usunięty ([ADR 0003](../../adr/0003-custom-gallery-virtualizer.md)); E2E: stop → komplet 16 ms we flingach, DOM stałe 211 węzłów | [05](05-custom-virtual-grid.md)
2026-11-07 | etap 07 | agent etapu 05 | in progress | Z etapu 05: pomiar pośredni `grid-baseline`, bramka wysokości do powtórki po aktualizacji WebKitGTK | [07](07-verification-and-docs.md)
2026-11-07 | etap 04 | agent etapu 05 | todo | Z etapu 05: interfejs `hasMore`/`onReachEnd` zostawiony do usunięcia; `getAssetAt` w memo komórek | [04](04-query-cache.md)
2026-11-07 | etap 06 | agent etapu 05 | todo | Z etapu 05: `ThumbnailImage` z jednym `<img>`, recykling kafli przy flingu | [06](06-thumb-protocol.md)
2026-11-07 | etap 08 | agent etapu 05 | todo | Z etapu 05: API modelu scrolla i pasek trybu okna dla scrubbera | [08](08-timeline-scrubber.md)
2026-11-07 | etap 03 | agent etapu 03 | done | Sort-key utrzymywany triggerami, porządek dokładny także z filtrami ([ADR 0004](../../adr/0004-trigger-maintained-gallery-sort-keys.md)); rebuild 1M bez filtrów 384 → 32 ms, exclude 671 → 420 ms, skan 20k bez spowolnienia; `list_assets` zostaje (odłożone) | [03](03-sort-key-and-indexes.md)
2026-11-07 | etap 07 | agent etapu 03 | in progress | Z etapu 03: pomiary pośrednie Rust i E2E; propozycja migracji 17 speców z `list_assets` | [07](07-verification-and-docs.md)
2026-11-07 | etap 04 | agent etapu 03 | todo | Z etapu 03: `lib.rs`, `commands/assets.rs`, `src/platform/tauri` i `ipc-contract.md` nietknięte; `list_assets` zostaje | [04](04-query-cache.md)
2026-11-07 | etap 04 | agent etapu 04 | in progress | Faza A: cache w miejscu z `version` (24 strony), refresh bez czyszczenia miniatur z fallbackiem starych stron, debounce `stale`, usunięte `hasMore`/`onReachEnd`; faza B (IPC) w toku | [04](04-query-cache.md)
2026-11-07 | etap 06 | agent etapu 04 | todo | Z etapu 04: refresh nie czyści już store miniatur; retencja obejmuje wiersze fallbacku | [06](06-thumb-protocol.md)
2026-11-07 | etap 07 | agent etapu 04 | in progress | Z etapu 04: cache 24 stron, `gallery.performance` (16 stron) nie wymusza już ewikcji | [07](07-verification-and-docs.md)
2026-11-07 | etap 04 | agent etapu 04 | done | Faza B: `anchorOffset` w `start_asset_query`, binarna komenda `get_asset_query_id_range` (f64 LE, [ADR 0005](../../adr/0005-binary-id-ranges-and-anchored-refresh.md)); Shift-click 100k 15–17 ms na desktopie, refresh głęboko w liście w 1 IPC bez pustych kafli | [04](04-query-cache.md)
2026-11-07 | etap 07 | agent etapu 04 | in progress | Z etapu 04: metryka `get_asset_query_id_range` w `grid_baseline.rs`; pomiary desktopowe Shift-click/refresh w notatkach etapu 04 | [07](07-verification-and-docs.md)
2026-11-07 | etap 06 | agent etapu 06 | done | `thumb://` z LIFO, prefetch `p=1`, cached-only `c=1`, kolejka frontendu usunięta ([ADR 0006](../../adr/0006-thumb-uri-scheme.md)); cold per kafel 354 → 286 ms, JPEG 24 MP 98 → 78 ms, wideo 202 → 48 ms | [06](06-thumb-protocol.md)
2026-11-07 | etap 07 | agent etapu 06 | in progress | Z etapu 06: pomiary pośrednie `grid-baseline` i dekodowania; harness `gallery-scale` z syntetycznym `thumb://` | [07](07-verification-and-docs.md)
2026-11-07 | etap 07 | orkiestrator | in progress | Fala D: kolumna „Po 01–06” (like/tag 0/20, rebuild 1M 32 ms, fling 16 ms, DOM 407 stałe, cold p50 ~290 ms), poprawione 3 regresje testów po 04/06, dokumentacja uzgodniona po 06; otwarte tematy 1–8; weryfikacja po fali E czeka | [07](07-verification-and-docs.md)
2026-11-07 | etap 08 | agent etapu 08 | done | Sekcje miesięcy w sesji (leniwe bez filtrów, w przebiegu z filtrami), `get_asset_query_sections`, `GalleryTimelineScrubber` obok paska pozycji ([ADR 0007](../../adr/0007-session-timeline-sections.md)); rebuild 1M 33–34 ms, pierwszy odczyt sekcji 29 ms; E2E skok do najstarszego miesiąca 88–134 ms (12k) | [08](08-timeline-scrubber.md)
