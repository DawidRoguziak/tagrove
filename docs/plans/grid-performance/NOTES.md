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
