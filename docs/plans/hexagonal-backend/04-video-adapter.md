# Etap 04: izolacja wideo (D1: cały stos w adapterze)

Fala B. Warstwa: Rust (player libmpv, zdarzenia libmpv, powierzchnia GTK, typy wejścia komend wideo, deklaracje modułów w `lib.rs`, test granicy) i dokumentacja. Kontrakt wire bez zmian.

Dokumentacja do przeczytania:
- [README planu](README.md) (sekcja 04 z notatką „Z etapu 01”, „Fale i równoległość”, „Zasady dla agentów”, D1);
- [ADR 0034](../../adr/0034-tauri-independent-backend-core.md), [ADR 0012](../../adr/0012-non-blocking-native-video-render.md), [ADR 0028](../../adr/0028-media-tools-open-only-the-file-they-are-given.md);
- `docs/architecture/system-overview.md` (startup order, tabela modułów, bezpieczeństwo z `PLAYER_OPTIONS`/`SCRIPT_OPTIONS`);
- `docs/subsystems/lightbox.md` (media presentation, wideo);
- `docs/architecture/ipc-contract.md` (`open_video`, `control_video`, kanał `VideoEvent`, `VideoBounds`, `VideoControlLabels`);
- `docs/development/testing.md` („Real desktop E2E”, specy wideo).

## Cel

Opróżnić `FRAMEWORK_ALLOWLIST` (3 → 0 wpisów) i `ADAPTER_IMPORT_ALLOWLIST` (1 → 0) w `src-tauri/tests/architecture_boundaries.rs`. Cały stos wideo (player libmpv, zdarzenia libmpv, powierzchnia GTK/OpenGL, typy `VideoBounds`/`VideoControlLabels`) trafia do `src-tauri/src/video/`, czyli do stałego zbioru adaptera `video/**`. Autoryzacja źródła po asset ID (`services/video_source_service.rs`) zostaje w rdzeniu. Zachowanie, model wątków GTK/libmpv (ADR 0012), opcje playera i narzędzi (ADR 0028), teksty błędów i kolejność startu się nie zmieniają.

## Problemy (potwierdzone w kodzie, commit `9d6ca08`)

- `FRAMEWORK_ALLOWLIST`: `services/video_events.rs` (`libmpv2`, `libmpv2_sys`), `services/video_player_service.rs` (`libmpv2`, `tauri::ipc::Channel`), `video_surface.rs` (`gtk`, `glib`, `libloading`, `libmpv2_sys`, `tauri::WebviewWindow`).
- `ADAPTER_IMPORT_ALLOWLIST`: `video_surface.rs:15-18` importuje `crate::commands::video::{VideoBounds, VideoControlLabels}`. Do tego dochodzi `:734` `crate::services::video_player_service::PlaybackSnapshot`.
- Importy przenoszonych plików:
  - `video_player_service.rs` z `crate` bierze tylko `super::video_events`;
  - `video_events.rs` nie importuje nic z `crate` (elementy `pub(super)`, 0 testów).
  - Przenoszone moduły nie potrzebują więc niczego z rdzenia. `video_source_service` woła tylko `commands/video.rs`.
- **Rdzeń sięga do przenoszonego modułu.** Test ADR 0028 `video_tools_refuse_reference_content_and_still_read_real_containers` w `thumbs.rs:1208` importuje `crate::services::video_player_service::test_support::{connection_arrived, loopback_listener, reference_fixtures, OpenWatch}`. Po przeniesieniu byłby to import `crate::video::…` w pliku rdzenia, czyli naruszenie reguły 3. Moduł `test_support` (`video_player_service.rs:693-815`) to czyste std + `libc` (loopback, inotify, fixtury) bez libmpv.
- Użytkownicy:
  - `lib.rs:11` (`pub mod video_surface`), `:113` (`services::video_player_service::VideoPlayerService::new()`), `:117` (`video_surface::setup`);
  - `services/mod.rs:13-14`;
  - `commands/video.rs`;
  - `dto_snapshot` w `architecture_boundaries.rs:977-988`.
  - Frontend nie wskazuje plików Rust.
- Testy: player 13, surface 6, `commands/video.rs` 3 (`validates_bounds`, `validates_and_trims_native_control_labels`, `parses_camel_case_track_control_fields`).
- Końce linii:
  - wszystkie edytowane pliki mają LF (0 linii CRLF);
  - wyjątek: `thumbs.rs` ma 393 linie CRLF, ale edytowana linia 1208 jest LF;
  - `bun run rust:fmt` to `cargo fmt --check`.

## Rdzeń i adapter

Adapter, `src-tauri/src/video/`. Nazwy plików: prefiks `video_` niesie już katalog, a `_service` to konwencja warstwy `services/` rdzenia. Nazwy typów (`VideoPlayerService`, `VideoEventClient`) zostają.

| Plik | Skąd | Zawartość |
| --- | --- | --- |
| `video/mod.rs` | nowy | `pub mod player; pub mod surface; pub mod types; mod mpv_events;` (widoczność jak dotąd: player i powierzchnia `pub`, zdarzenia prywatne) |
| `video/player.rs` | `git mv services/video_player_service.rs` | `VideoPlayerService`, worker, `PLAYER_OPTIONS`/`SCRIPT_OPTIONS`, DTO kanału `VideoEvent`, `VideoEventPayload`, `PlaybackSnapshot` |
| `video/mpv_events.rs` | `git mv services/video_events.rs` | `NativeEvent`, `VideoEventClient` (klient zdarzeń libmpv) |
| `video/surface.rs` | `git mv video_surface.rs` | nakładka GTK, GLArea, renderowanie OpenGL libmpv, natywne kontrolki |
| `video/types.rs` | wycięte z `commands/video.rs` | `VideoBounds`, `VideoControlLabels`, ich `validate` i dwa testy walidacji |

- `mpv_events`, a nie `events`: plik opakowuje API zdarzeń klienta libmpv (`mpv_handle`), a nie DTO kanału `VideoEvent` ani eventy aplikacji z `adapter/events.rs`.
- `types.rs`: atrybuty serde bez zmian. Jedyna zmiana widoczności to `VideoControlLabels::validate` z prywatnej na `pub(crate)`, bo woła ją `commands/video.rs`.
- `VideoControl` zostaje w `commands/video.rs`. To wejście komendy `control_video`, mapowane na `PlayerCommand` w handlerze, i nie ma innego użytkownika.
- Bez portu `VideoEventSink` (D1): `tauri::ipc::Channel<VideoEvent>` zostaje w `ActiveSession` i `WorkerCommand::Open` playera.

**Gdzie żyją DTO kanału (`VideoEvent`, `VideoEventPayload`, `PlaybackSnapshot`):** w `video/player.rs`, w adapterze.
- Producent (worker playera) i konsument (`Channel<VideoEvent>` komendy `open_video`) są w adapterze, a rdzeń nigdy nie tworzy tych typów.
- Plik przechodzi dosłownie, bez rozcinania.
- `dto_snapshot` zamraża JSON niezależnie od miejsca typu; zmienia się tylko ścieżka importu.
- Odrzucone: `models.rs` rdzenia. Typy są wolne od Tauri (tylko serde), ale nie mają użytkownika w rdzeniu. Przeniesienie wprowadziłoby do rdzenia kształt wire adaptera pod port, który D1 odrzuca.

**Co przechodzi do rdzenia:** `test_support` → nowy plik `src-tauri/src/test_support.rs`, w `lib.rs` jako `#[cfg(test)] pub(crate) mod test_support;`.
- Treść jest przeniesiona dosłownie. Doc komentarz modułu mówi, że fixtury są wspólne dla testów playera i `thumbs.rs`.
- Player importuje fixtury z rdzenia (`crate::test_support`), co jest dozwolonym kierunkiem adapter → rdzeń. `thumbs.rs:1208` zmienia tylko ścieżkę.
- Odrzucone warianty:
  - `thumbs.rs` na `ADAPTER_IMPORT_ALLOWLIST`: allowlista może tylko maleć;
  - przeniesienie testu ffmpeg/ffprobe do `video/`: testuje narzędzia rdzenia (`thumbs.rs`);
  - kopia fixtur w obu miejscach: dwie wersje testu bezpieczeństwa ADR 0028 zaczęłyby się rozjeżdżać.

**Rdzeń (bez zmian):** `services/video_source_service.rs` (kanoniczna ścieżka po asset ID, zwykły plik) i `thumbs.rs` (`LOCAL_VIDEO_INPUT_ARGS` ffmpeg/ffprobe). Komendy, klucze argumentów, pola odpowiedzi, kanał i eventy też się nie zmieniają.

**Test granicy:**
- Obie allowlisty zostają jako puste stałe (`&[]`), razem z `allowlist_entries_exist` i `allowlist_entries_are_still_needed`:
  - ponowny wpis nadal czerwieni test (plik nie istnieje, plik jest w stałym zbiorze adaptera albo czysty plik rdzenia);
  - mechanizm może zniknąć w etapie 08 razem z grepowymi regułami 1 i 3, gdy granicę przejmie kompilator.
  - Odrzucone: usunięcie stałych i obu testów już teraz. Etapy 05–07 nie mogłyby wtedy sprawdzić, że nikt nie wraca do allowlisty.
- `"video_surface"` znika z `ADAPTER_MODULES` i `DB_FORBIDDEN_MODULES`, bo modułu już nie ma, a `"video"` zostaje. Plik `video_surface.rs` odtworzony w korzeniu i tak złapie reguła 1.
- Nowy test `video_does_not_import_commands` (reguła 3, strona adaptera): pliki `video/**` nie odwołują się do `crate::commands` (także przez `super::super::`).
  - Reguła 3 sprawdza tylko pliki rdzenia, więc bez tej asercji wyjście `rg 'commands::' src-tauri/src/video` nie byłoby zamrożone.
  - Kierunek komendy → `video` jest dozwolony.
- Nowego ADR nie ma: etap wykonuje D1 z ADR 0034, a powyższe decyzje to szczegóły zapisane tutaj razem z alternatywami.

## Kroki

1. Ten plik; status 04 w README → `in progress`.
2. Pomiar bazowy: `--lib` i `--test architecture_boundaries`, lista testów `--lib` (`-- --list`), liczba CRLF w `thumbs.rs`.
3. `git mv` trzech plików do `src-tauri/src/video/`; `video/mod.rs`.
4. `video/player.rs`: `super::video_events` → `super::mpv_events`; blok `test_support` → `src/test_support.rs`; w `mod tests` `super::test_support` → `crate::test_support`.
5. `video/surface.rs`: importy z `crate::video::{player, types}` (dwa miejsca).
6. `video/types.rs` z `commands/video.rs`; `commands/video.rs`: importy i `crate::video_surface::` → `crate::video::surface::`, ciała bez zmian.
7. `services/mod.rs`: bez `video_events` i `video_player_service`.
8. `lib.rs`: deklaracje modułów (`video`, `test_support`) i dwie ścieżki w tym samym miejscu `setup`.
9. `thumbs.rs:1208`: tylko ścieżka importu, przez `sed` na jednej linii (CRLF 393 przed i po).
10. `architecture_boundaries.rs`: puste allowlisty, listy modułów, `video_does_not_import_commands`, importy `dto_snapshot`.
11. `rustfmt` tylko na edytowanych plikach LF, potem `bun run rust:fmt`.
12. Dokumentacja (lista w [Pliki](#pliki)).

## Pliki

- Nowe:
  - `src-tauri/src/video/mod.rs`, `src-tauri/src/video/types.rs`, `src-tauri/src/test_support.rs`;
  - ten plik.
- Przeniesione (`git mv`, potem edycja importów):
  - `services/video_player_service.rs` → `video/player.rs`;
  - `services/video_events.rs` → `video/mpv_events.rs`;
  - `video_surface.rs` → `video/surface.rs`.
- Zmienione:
  - `src-tauri/src/commands/video.rs`, `src-tauri/src/services/mod.rs`;
  - `src-tauri/src/lib.rs` (deklaracje modułów i ścieżki);
  - `src-tauri/src/thumbs.rs` (jedna linia w teście);
  - `src-tauri/tests/architecture_boundaries.rs`;
  - `docs/architecture/system-overview.md`: tabela modułów, granica i allowlisty, „Managed state” (`video/mpv_events.rs`), bezpieczeństwo (`PLAYER_OPTIONS`/`SCRIPT_OPTIONS` w `video/player.rs`);
  - `docs/subsystems/lightbox.md`: entry points i testy Rust;
  - `docs/architecture/ipc-contract.md`: ścieżka testów playera;
  - `docs/development/testing.md`: testy powierzchni i fixtury ADR 0028;
  - `CLAUDE.md` (Layout);
  - `.claude/rules/lightbox-video.md` (`paths:` i treść), `.claude/rules/backend-core.md` (`paths:` i kolejność startu);
  - `.claude/skills/audit/references/batches.json` (B6a, B6b);
  - `README.md` (status, „Egzekucja granicy”, sekcje 07 i 08: `Z etapu 04`) i `NOTES.md` planu.
- Bez zmian:
  - `COMMAND_SIGNATURES`, `REGISTERED_COMMANDS`, JSON w `dto_snapshot`, nazwy eventów;
  - `Cargo.toml`, `Cargo.lock`, frontend;
  - ADR 0012, 0028 i 0034 (historyczne);
  - `docs/review/**` (historyczne raporty audytu);
  - `.claude/skills/tauri-testing/skills/tauri-rust-tests/SKILL.md`: wskazuje `parses_camel_case_track_control_fields` w `commands/video.rs`, a test tam zostaje.

## Testy

- Stan wyjściowy (`9d6ca08`): patrz notatki.
- Przeniesione z kodem (nazwy bez zmian):
  - testy playera → `video::player::tests`;
  - testy powierzchni → `video::surface::tests`;
  - `validates_bounds`, `validates_and_trims_native_control_labels` → `video::types::tests`.
  - `parses_camel_case_track_control_fields` zostaje w `commands::video::tests`, a test ffmpeg w `thumbs::tests`.
- Nowy: `architecture_boundaries::video_does_not_import_commands`.
- Komendy:
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib`;
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib -- video:: thumbs::tests::video_tools_refuse`;
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`;
  - `bun run rust:fmt`, `bun run rust:clippy`, `bun run test:backend`, `git diff --check`;
  - `python3 -I -m unittest` w `.claude/skills/audit/scripts`.
- Próby mutacyjne (każda cofnięta): patrz notatki.
- Desktop E2E (jeden spec naraz, prywatny Xvfb, tymczasowe `XDG_*`, `MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"`):
  - `native-video-escape` (`MEDIATAGGER_NATIVE_VIDEO=1`);
  - `video-speed` (`MEDIATAGGER_NATIVE_VIDEO=1`);
  - `app.smoke`.
- Ręcznie: `/verify-mediatagger` (otwarcie wideo w lightboxie, kontrolki, Escape, fullscreen) ze zrzutem ekranu.

## Kryteria akceptacji

- `FRAMEWORK_ALLOWLIST` i `ADAPTER_IMPORT_ALLOWLIST` puste; `allowlist_entries_are_still_needed`, reguły 1–3 i `video_does_not_import_commands` zielone.
- `rg 'commands::' src-tauri/src/video` puste.
- `rg 'video_surface|video_player_service|video_events' src-tauri/src src-tauri/tests` puste.
- Snapshot wire (komendy, sygnatury, JSON DTO, eventy) bez zmian.
- Liczba testów `--lib` bez spadku; zbiór nazw testów różni się tylko ścieżką modułu.
- `git diff src-tauri/src/lib.rs` to tylko deklaracje modułów i dwie ścieżki w tym samym miejscu `setup`.
- `git diff -M` pokazuje trzy pliki wideo jako rename.
- `thumbs.rs` ma nadal 393 linie CRLF.

## Ryzyka

- **Model wątków GTK/libmpv (ADR 0012).** Kod jest przeniesiony dosłownie: zmieniają się tylko ścieżki `use`. Realną ścieżkę renderowania pokrywa tylko desktop E2E.
- **Opcje playera (ADR 0028).** `PLAYER_OPTIONS`/`SCRIPT_OPTIONS` przechodzą razem z plikiem. Fixtury referencji są teraz w rdzeniu, a oba testy (player i ffmpeg) z nich korzystają.
- **Mieszane końce linii w `thumbs.rs`.** Edycja jednej linii przez `sed`, a mutacje przez `open(path, newline='')`.

## Status

done (2026-11-14). Commit czeka na akceptację użytkownika.

## Notatki agentów

### 2026-11-14 — agent etapu 04
- Zrobione:
  - `git mv` trzech plików do `src/video/` (`player.rs`, `mpv_events.rs`, `surface.rs`), nowe `video/mod.rs` i `video/types.rs` (`VideoBounds`, `VideoControlLabels`, `validate` i dwa testy z `commands/video.rs`). Kod przeniesiony dosłownie: zmieniły się tylko ścieżki `use`, widoczność `VideoControlLabels::validate` (`pub(crate)`) i jeden komentarz (`video_surface` → `video::surface` w doc komentarzu `PLAYER_OPTIONS`). `git diff -M` pokazuje trzy pliki jako rename;
  - `test_support` (fixtury ADR 0028) → rdzeń `src/test_support.rs` (`#[cfg(test)]`), treść bez zmian (tylko wcięcie o poziom mniej, doc komentarz modułu jako `//!`); `thumbs.rs:1208` i testy playera importują `crate::test_support`;
  - `lib.rs`: deklaracje `video` i `test_support` oraz dwie ścieżki w tym samym miejscu `setup`; kolejność startu bez zmian;
  - `architecture_boundaries.rs`: `FRAMEWORK_ALLOWLIST` 3 → 0, `ADAPTER_IMPORT_ALLOWLIST` 1 → 0 (stałe zostają puste), `video_surface` usunięty z list modułów, nowy test `video_does_not_import_commands`, importy `dto_snapshot` (JSON bez zmian);
  - dokumentacja: system-overview, lightbox, ipc-contract, testing, `CLAUDE.md`, reguły `lightbox-video`/`backend-core`, batch audytu B6a/B6b, README planu (status, „Egzekucja granicy”, `Z etapu 04` w 07 i 08).
- Odkryte:
  - `thumbs.rs:1208` (rdzeń) importował fixtury z `video_player_service::test_support`; bez przeniesienia fixtur do rdzenia przeniesienie playera łamałoby regułę 3 (próba 7);
  - reguła 3 sprawdza tylko pliki rdzenia, więc import `commands` z `video/` łapie dopiero nowy test (próba 3);
  - `rustfmt` uruchomiony na pliku z `mod` idzie za deklaracjami modułów: na `lib.rs` i `services/mod.rs` dotknąłby plików CRLF (`thumbs.rs`, `thumb_service.rs`, `thumb_scheduler.rs`). Formatowane były tylko `video/mod.rs` (z dziećmi), `test_support.rs`, `commands/video.rs` i test granicy; `cargo fmt --check` zielony;
  - `ipc-contract.md` przypisywał walidację bounds/labels testom `commands/video.rs`; teraz wskazuje `video/types.rs`;
  - `SKILL.md` `tauri-rust-tests` wskazuje `parses_camel_case_track_control_fields` w `commands/video.rs`, który tam został: bez zmian;
  - `libc` w rdzeniu używają `services/asset_mutation_service.rs` i `test_support.rs` (notatka dla 08).
- Zostało: commit po akceptacji.
- Testy:
  - `--lib`: przed 364 passed, 2 ignored (366 nazw); po 364 passed, 2 ignored; zbiór 366 nazw identyczny poza ścieżką modułu (`services::video_player_service` → `video::player`, `video_surface` → `video::surface`, dwa testy walidacji `commands::video` → `video::types`);
  - `cargo test … --test architecture_boundaries`: przed 23 passed, 1 ignored; po 24 passed, 1 ignored;
  - `rg 'commands::' src-tauri/src/video` puste; `rg 'video_surface|video_player_service|video_events' src-tauri/src src-tauri/tests` puste; `thumbs.rs` 393 linie CRLF przed i po (`thumb_service.rs` 423, `thumb_scheduler.rs` 459 bez zmian);
  - próby mutacyjne (9), każda czerwona zgodnie z oczekiwaniem i cofnięta (zapis binarny, `md5sum` po przywróceniu):
    - 1. `use tauri::ipc::Channel;` w `services/video_source_service.rs` → `core_does_not_use_the_gui_runtime` (`src/services/video_source_service.rs:1: rule 1: tauri`);
    - 2a. wpis `("services/video_player_service.rs", "04")` → `allowlist_entries_exist` („names a missing file”);
    - 2b. wpis `("video/player.rs", "04")` → `allowlist_entries_exist` („already in the permanent adapter set”);
    - 2c. wpis `("services/video_source_service.rs", "04")` w `ADAPTER_IMPORT_ALLOWLIST` → `allowlist_entries_are_still_needed` („no longer imports the adapter … (stage 04 done)”);
    - 3. `use crate::commands::video::VideoControl;` w `video/surface.rs` → `video_does_not_import_commands` (`src/video/surface.rs:15: rule 3: crate::commands`);
    - 4. `use crate::video::types::VideoBounds;` w `video_source_service.rs` → `core_does_not_import_the_adapter` (reguła 3);
    - 5. `#[serde(rename = "w")]` na `VideoBounds::width` → `dto_snapshot::video_inputs` („missing field `w`”);
    - 6. `VideoControlLabels` bez `rename_all = "camelCase"` → `dto_snapshot::video_inputs` („missing field `playback_rate`”);
    - 7. `thumbs.rs:1208` z importem `crate::video::player::test_support` → `core_does_not_import_the_adapter` (`src/thumbs.rs:1208: rule 3: crate::video`);
  - `bun run rust:fmt` → pass; `bun run rust:clippy` → pass; `bun run test:backend` → pass (lib 364, `architecture_boundaries` 24, pozostałe cele zielone);
  - `python3 -I -m unittest` (skrypty audytu) → pass (31);
  - `git diff --check` → pass;
  - desktop E2E (prywatny Xvfb, tymczasowe `XDG_*`, `MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"`, jeden spec naraz):
    - `native-video-escape` (`MEDIATAGGER_NATIVE_VIDEO=1`, 1440x1100) → 8 passing;
    - `video-speed` (`MEDIATAGGER_NATIVE_VIDEO=1`, 1440x1000) → 7 passing;
    - `app.smoke` → 4 passing;
  - `/verify-mediatagger` (`app:control`, zrzuty w ignorowanym `artifacts/app-control/`):
    - otwarcie MP4 w lightboxie: obraz i natywne kontrolki GTK widoczne, sesja 1 gra;
    - natywny klik pauzy → `data-native-paused=true`;
    - popover prędkości → 1.5× (`data-native-rate=1.5`), wznowienie → `paused=false`;
    - natywny fullscreen → układ pełnoekranowy (`data-native-fullscreen`);
    - natywny Escape → wyjście z fullscreen, drugi Escape → zamknięcie lightboxa, powierzchnia schowana;
    - ponowne otwarcie drugiego wideo → sesja 2 gra, Close preview zamyka;
    - konsola frontendu bez błędów i ostrzeżeń, `process.log` bez paniki i `fail_render`; sesja zatrzymana, katalog tymczasowy usunięty.
