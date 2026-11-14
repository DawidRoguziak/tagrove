# Dziennik przekazań

Jedna linia na wpis, najnowsze na dole. Szczegóły trafiają do sekcji `## Notatki agentów` w pliku etapu.

Format: `YYYY-MM-DD | etap NN | rola | status | jedno zdanie | link do etapu`

---

2026-11-13 | — | planowanie | plan | Analiza backendu (3× Explore Sonnet, 1× Plan Opus): Tauri w 13 plikach, rdzeń prawie czysty; 9 etapów w 5 falach; D1 = wideo w adapterze, D2 = crate rdzenia jako cel | [README](README.md)
2026-11-14 | etap 01 | agent etapu 01 | done | Cel `architecture_boundaries` (reguły 1–3, reguła 4 jako ignore, snapshot komend/sygnatur/DTO/eventów), ADR 0034 Proposed; poprawka: 14 plików (`video_events.rs`) i allowlista reguły 3 (`video_surface.rs`) | [01](01-boundary-guard.md)
2026-11-14 | etap 02 | agent etapu 02 | done | `impl ProgressSink for AppHandle` → `adapter/events.rs`, `clear_library_data_service` na `&impl ProgressSink`, asercja `no-store` w testach `thumb_protocol`; allowlista reguły 1: 7 → 5; uwaga dla 08: reguła sierot | [02](02-progress-sink.md)
