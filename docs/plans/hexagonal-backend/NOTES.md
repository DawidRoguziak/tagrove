# Dziennik przekazań

Jedna linia na wpis, najnowsze na dole. Szczegóły trafiają do sekcji `## Notatki agentów` w pliku etapu.

Format: `YYYY-MM-DD | etap NN | rola | status | jedno zdanie | link do etapu`

---

2026-11-13 | — | planowanie | plan | Analiza backendu (3× Explore Sonnet, 1× Plan Opus): Tauri w 13 plikach, rdzeń prawie czysty; 9 etapów w 5 falach; D1 = wideo w adapterze, D2 = crate rdzenia jako cel | [README](README.md)
2026-11-14 | etap 01 | agent etapu 01 | done | Cel `architecture_boundaries` (reguły 1–3, reguła 4 jako ignore, snapshot komend/sygnatur/DTO/eventów), ADR 0034 Proposed; poprawka: 14 plików (`video_events.rs`) i allowlista reguły 3 (`video_surface.rs`) | [01](01-boundary-guard.md)
2026-11-14 | etap 02 | agent etapu 02 | done | `impl ProgressSink for AppHandle` → `adapter/events.rs`, `clear_library_data_service` na `&impl ProgressSink`, asercja `no-store` w testach `thumb_protocol`; allowlista reguły 1: 7 → 5; uwaga dla 08: reguła sierot | [02](02-progress-sink.md)
2026-11-14 | etap 03 | agent etapu 03 | done | `StartupScanState`, `StartupScanRequest` i `run_startup_scan` (z testami) → `adapter/startup_scan.rs`, dosłownie; allowlista reguły 1: 5 → 4; notatki dla 06 (closure skanu do use case'u) i 07 (`open_storage` zwraca `Vec<ScanRoot>`) | [03](03-startup-scan.md)
2026-11-14 | etap 05 | agent etapu 05 | done | `thumb_protocol.rs` bez Tauri (`ThumbReply`, porty `ThumbResponder` jako closure i `ThumbHost` jako trait), `adapter/thumb_scheme.rs` (handler, `TauriThumbHost`, konwersja na HTTP); allowlista reguły 1: 4 → 3; notatki dla 07 (okno 503) i 08 (newtype, brak E0117) | [05](05-thumb-scheme.md)
2026-11-14 | etap 04 | agent etapu 04 | done | Stos wideo → `src/video/` (`player`, `mpv_events`, `surface`, `types`; `git mv`, dosłownie), fixtury ADR 0028 → rdzeń `test_support.rs`; allowlisty reguł 1 i 3 puste (3 → 0, 1 → 0), nowy test `video_does_not_import_commands`; notatki dla 07 (faza wideo w `setup`) i 08 (zależności GTK/libmpv tylko w adapterze, feature dla fixtur) | [04](04-video-adapter.md)
2026-11-14 | etap 06 | agent etapu 06 | done | `*_service` i ciała workflowów z `commands/{assets,scan,thumbs,import_export}.rs` → `usecases/*` (dosłownie, 13 testów z kodem), komendy cienkie, reguła 4 bez `#[ignore]` i z tokenem `locks`, `lib.rs` +1 linia `pub mod usecases;`; notatki dla 07 (bootstrap bez use case'ów), 08 (`usecases/` do rdzenia, reguła 4 zostaje grepem, `&Arc<DatabaseRuntime>`) i 09 (evale `review-changes`, `open_video`, teksty błędów bez strażnika) | [06](06-use-cases.md)
2026-11-14 | etap 07 | agent etapu 07 | done | Fazy rdzenia startu → `app/bootstrap.rs` (`open_storage`, `resolve_ffmpeg_path`, `start_workers`, dosłownie, `Box<dyn Error>`), `lib.rs` tylko składa (`manage(AppState)` ostatni), 7 testów bootstrapu na tempdir; luki bez strażnika: kolejność `manage` (tylko E2E), `worker_spawn_failures`, import `usecases`; notatki dla 08 i 09 | [07](07-bootstrap.md)
