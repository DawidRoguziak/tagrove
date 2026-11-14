# Etap 09: dokumentacja, reguły, skille, ADR 0034 Accepted, pełny E2E

Fala E. Warstwa: `.claude/**` (reguły, skille, agenci, evale), dokumentacja, ADR 0034 oraz małe zmiany Rust bez zmiany zachowania (use case `open_video`, strażnicy w `architecture_boundaries.rs`, testy tekstów błędów). Kontrakt wire, schemat i `Cargo.lock` bez zmian.

Dokumentacja do przeczytania:
- [README planu](README.md) (sekcja 09 z notatkami „Z etapu 06”, „Z etapu 07”, „Z etapu 08”, „Zasady dla agentów”, „Protokół notatek”);
- [08](08-crate-split.md) (notatki agenta), [07](07-bootstrap.md) (luki bez strażnika), [06](06-use-cases.md) (próby E1–E5, L1, R1);
- [ADR 0034](../../adr/0034-tauri-independent-backend-core.md), [docs/adr/README.md](../../adr/README.md) (cykl życia);
- `CLAUDE.md`, `docs/architecture/system-overview.md`.

## Cel

Wszystko wokół kodu opisuje końcowy układ dwóch pakietów: reguły ścieżkowe, `HOT_PATHS`, batche audytu, evale, skille, strony podsystemów i system-overview. Luki wskazane przez etapy 06 i 07 są domknięte strażnikiem albo opisane jako świadome. ADR 0034 ma status `Accepted`. Pełny desktop E2E (wszystkie specy, także opt-in) i `tauri:dev` są zielone.

## Stan wyjściowy (HEAD `df46484`)

- Globy `paths:` 9 reguł, `HOT_PATHS`, 33 globy `batches.json` i przykłady w skillach wskazują `src-tauri/src/<moduł rdzenia>`, czyli pliki, których już nie ma. Reguły nie ładują się dla rdzenia, a batche audytu B1–B11 tracą rdzeń po cichu.
- `test_batches_reference_existing_files` jest czerwony od etapu 08 (`B3a matches no files`; B3a ma tylko pliki rdzenia).
- Łatki evali `01`, `02`, `05`, `06` nie stosują się na HEAD (01, 02 i 06 sprzed etapu 06; 05 po zmianach `GalleryGrid.tsx`).
- Strony podsystemów i `ipc-contract.md` mają 42 stare ścieżki (także linki względne).
- Pliki CRLF/mixed: lista z liczbami i md5 w pomiarze bazowym (scratchpad sesji). Wszystkie pliki zmieniane w tym etapie mają LF.

## Decyzje

1. **`open_video` (D9-1, decyzja użytkownika).** Odczyt źródła przechodzi do nowego use case'u rdzenia `usecases/video.rs::open_video_service(&Arc<DatabaseRuntime>, asset_id) -> AppResult<PathBuf>`. Ciało jest przeniesione dosłownie: `admit()?.connection()?` + `video_source_service::resolve_video_path`. Komenda zostaje w adapterze (D1) z tym samym `spawn_blocking` i tymi samymi `map_err`. Reguła 4 dostaje token `admit`, więc żadna komenda nie otworzy połączenia sama.
2. **Luki z etapu 07 (D9-2, decyzja użytkownika):**
   - (a) nowy skan `app_state_is_managed_last_in_setup`: w `lib.rs` adaptera `start_workers` występuje dokładnie raz, w argumencie ostatniego `manage(` closure `.setup(`, a za nim jest tylko `Ok(())`;
   - (c) `bootstrap_does_not_import_usecases`;
   - (b) `worker_spawn_failures` zostaje świadomą luką (porażki spawnu nie da się wywołać bez portu);
   - M3b (odzysk operacji plikowych po snapshotach) także zostaje luką, jak w 07.
3. **Teksty błędów (D9-3, decyzja użytkownika).** Testy porównują literały: `"Invalid query owner ID"`, `"At most 256 asset IDs are allowed"`, `"Invalid hasNoTags meta filter"`, `"Folder path does not exist or is not a directory"` w `scan_folder_service`, wartość `LIBRARY_BUSY_ERROR`. Kolejność blokad w `with_scan_and_thumb_lock` (L1) i kolejność walidacja → `begin_request` (R1) zostają świadomymi lukami.
4. **ADR 0034.** Status `Proposed` pozwala poprawić treść; `docs/adr/README.md` zabrania przepisywania tylko Accepted. Treść dostaje stan końcowy (nowe strażniki, D1 z `usecases::video`, lista luk), a status zmienia się na `Accepted` w tym samym commicie.
5. **Strażnicy dryfu ścieżek** w testach Pythona:
   - każdy wzorzec `HOT_PATHS` i każdy glob `paths:` reguł trafia w śledzony plik (`test_collect_scope.py`);
   - każdy glob batcha audytu trafia w plik, a każdy plik `.rs` (poza `mod.rs`) z `src` obu pakietów należy do batcha (`test_audit_scripts.py`).
6. **Evale.** Łatki 01, 02, 05 i 06 są generowane `git diff`-em w jednorazowym klonie (bez worktree) z tym samym defektem w nowym miejscu, a `base_commit` przechodzi na `df46484`.
7. **`recon-candidates.json`.** Ścieżki przechodzą na obecne pliki; numery linii zostają wskazówką albo znikają tam, gdzie kod przeszedł do innego pliku.

## Kroki

1. Pomiar bazowy, ten plik, status `in progress`.
2. Kod: `usecases/video.rs`, `commands/video.rs`, reguła 4, nowi strażnicy, testy tekstów; `rustfmt` tylko na edytowanych plikach LF.
3. `.claude/**`: reguły, `collect_scope.py` z testem, evale, audyt z testem, skille, agent `review-rust`.
4. Dokumentacja: ścieżki, system-overview, `ipc-contract.md`, `testing.md`, ADR 0034 i indeks.
5. Weryfikacja i próby mutacyjne.
6. Pełny desktop E2E i `tauri:dev`.
7. `/review-changes HEAD --save`, notatki, status.

## Pliki

- Nowe:
  - `src-tauri/crates/media_tagger_core/src/usecases/video.rs`;
  - ten plik.
- Zmienione, kod:
  - `src-tauri/crates/media_tagger_core/src/usecases/{mod,assets,scan}.rs` (moduł `video`, testy tekstów);
  - `src-tauri/src/commands/video.rs` (use case zamiast `admit()` + `video_source_service`);
  - `src-tauri/tests/architecture_boundaries.rs` (token `admit`, trzy nowe testy).
- Zmienione, `.claude/**`:
  - `rules/{backend-core,data-safety,database,ipc-contract,lightbox-video,scanning,search-tags,testing-e2e,thumbnails}.md`;
  - `agents/review-rust.md`;
  - `skills/review-changes/{scripts/collect_scope.py,scripts/test_collect_scope.py,evals/README.md,evals/expected.json,evals/patches/0{1,2,5,6}-*.patch,references/signal-bar.md}`;
  - `skills/audit/{references/batches.json,references/finding-format.md,references/recon-candidates.json,scripts/test_audit_scripts.py}`;
  - `skills/tauri-testing/skills/{tauri-rust-tests,tauri-test-strategy}/SKILL.md`.
- Zmienione, dokumentacja:
  - `CLAUDE.md` (`usecases/` z `video`);
  - `docs/architecture/{system-overview,ipc-contract}.md`;
  - `docs/development/testing.md`;
  - `docs/subsystems/{database,scanning-and-indexing,library-query-and-gallery,thumbnails,search-tags-and-media-groups,lightbox,data-safety-and-portability}.md`;
  - `docs/adr/0034-tauri-independent-backend-core.md` (Accepted), `docs/adr/README.md`;
  - `README.md` i `NOTES.md` planu.
- Bez zmian: `Cargo.toml`, `Cargo.lock`, `generate_handler!`, `COMMAND_SIGNATURES`, `REGISTERED_COMMANDS`, `dto_snapshot`, eventy, frontend, `e2e/*`, pliki CRLF.

## Testy

- `python3 -I -m unittest` w `.claude/skills/review-changes/scripts` i `.claude/skills/audit/scripts`.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`, `-p media_tagger_core usecases`, `bun run test:backend`, lista testów przed i po.
- `bun run quality`, `bun run privacy:check`, `bun run test:privacy`, `git diff --check`, linki w zmienionych `.md`, liczby CRLF i md5.
- Desktop E2E: 27 speców, jeden naraz, każdy wywołany jawnie; `tauri:dev`.

## Kryteria akceptacji

- `rg 'src-tauri/src/(db|services|usecases|models|error|indexer|thumbs|utils|test_support|app/(bootstrap|state|locks|instance_lock))' docs .claude CLAUDE.md` puste poza `docs/plans/**` i `docs/adr/**`.
- Testy Pythona obu skilli zielone, z nowymi strażnikami dryfu.
- 6/6 łatek evali stosuje się na końcowym drzewie.
- `architecture_boundaries` zielony; tabele snapshotu wire bez zmian; liczba testów tylko rośnie.
- `quality`, `test:backend`, `privacy:check` i `test:privacy` zielone; `git diff --check` czysty; liczby CRLF i md5 jak w bazie.
- E2E i `tauri:dev` zielone.
- ADR 0034 `Accepted`.

## Ryzyka

- **Strażnik „manage last” bywa kruchy**: odrzuca też poprawny kształt `let state = start_workers(…)?; app.manage(state)`. Komunikat wyjaśnia inwariant, a autotest pokazuje dozwolony kształt.
- **Długie specy** (`gallery-100k`, `gallery.performance`): progi wydajności zależą od maszyny.
- **`rustfmt`** formatuje moduły potomne: uruchamiany tylko na plikach-liściach LF.

## Status

done (2026-11-14). Commit czeka na akceptację użytkownika.

## Notatki agentów

### 2026-11-14 — agent etapu 09
- Zrobione: decyzje 1–7 jak wyżej. Dodatkowo:
  - `HOT_PATHS` dostał `src-tauri/src/adapter/thumb_scheme.rs` (handler `thumb://` wołany per kafelek);
  - `batches.json`: B5 + `services/export_target.rs`, B6a + `usecases/video.rs`, B11 + `lib.rs` i `Cargo.toml` rdzenia, B12 + `app/*.rs` rdzenia.
- Odkryte:
  - `test_batches_reference_existing_files` był czerwony od etapu 08 (B3a bez plików), a notatki 08 nie wymieniały testów Pythona audytu; teraz zielony.
  - Łatka `05-selection-scan` też przestała się stosować (zmiany `GalleryGrid.tsx` po `76ff5b1`); wygenerowana ponownie razem z 01, 02 i 06.
  - Łatka 02 musiała zachować kontrolę „busy” z ADR 0022, bo `delete_asset_service` używa dziś `try_with_scan_and_thumb_lock`; defekt to blokujące `scan_lock` po wyłącznym `thumb_lock` za sprawdzeniem `try_lock`.
  - Skrypt podmiany ścieżek objął też ignorowane, nieśledzone raporty `docs/review/20261108-1145/` (132 zamiany). Run sprzed crate'a nie mógł zawierać ścieżki crate'a, więc cofnięte dokładną zamianą odwrotną (liczby zgodne, 0 wystąpień `media_tagger_core` po cofnięciu).
  - Pierwszy przebieg prób skanera dał fałszywe „zielone”: skrypt wybrał starszą binarkę `architecture_boundaries` (25 testów, bez nowych strażników). Po wyborze najnowszej i sprawdzeniu `--list` wszystkie próby czerwone.
  - Strażnik `app_state_is_managed_last_in_setup` widzi tylko closure `.setup(`; `manage` w funkcji pomocniczej by przeoczył (uwaga soczewki tooling, poniżej progu).
- Zostało: commit po akceptacji.
- Testy:
  - lista testów (`-- --list --include-ignored`): 444 → 450, żaden nie zniknął; nowe `app_state_is_managed_last_in_setup`, `app_state_manage_check_accepts_only_the_last_step`, `bootstrap_does_not_import_usecases`, `usecases::assets::tests::validation_errors_keep_their_wire_text`, `usecases::scan::tests::missing_folder_is_rejected_with_its_wire_text`, `usecases::video::tests::open_video_service_resolves_an_authorized_source_and_keeps_the_error_text`;
  - `bun run test:backend` → pass (`media_tagger` lib 31, `media_tagger_core` lib 343 + 2 ignored, `architecture_boundaries` 26, `backend_e2e` 1, `backend_integration` 22 + 1 ignored, `backend_runtime` 6 + 1 ignored, `tag_vocabulary` 6, `thumbnail_bulk` 2, `thumbnail_clear` 2, diagnostyki ignorowane bez zmian);
  - `python3 -I -m unittest discover`: `review-changes/scripts` 9 → 11 OK, `audit/scripts` 31 (1 FAIL) → 32 OK;
  - `bun run quality` → pass; `bun run privacy:check` → pass (244 commity); `bun run test:privacy` → pass;
  - `git diff --check`: tylko puste linie kontekstu (` `) w plikach `.patch` evali, wymagane przez format łatki (stare łatki miały to samo);
  - liczby CRLF i md5 wszystkich plików crlf/mixed identyczne z bazą; wszystkie zmienione pliki LF; `rustfmt` tylko na edytowanych plikach-liściach, `cargo fmt --all -- --check` czysty;
  - linki względne i kotwice w 26 zmienionych `.md` (skrypt w scratchpadzie) → 0 problemów; kryterium `rg` puste poza `docs/plans/**` i `docs/adr/**`; 6/6 łatek evali → `git apply --check` OK na końcowym drzewie;
  - próby mutacyjne (skrypt binarny, każda cofnięta z kontrolą md5; po wszystkich md5 144 plików zgodne ze zrzutem sprzed prób), wszystkie czerwone:
    - M7 `manage(AppState)` przed fazą wideo, M8 przed `manage(player)`, M8b `let state = start_workers(..)?; app.manage(state)` → `app_state_is_managed_last_in_setup`;
    - M11 `use crate::usecases;` w `bootstrap.rs` → `bootstrap_does_not_import_usecases`;
    - `database.admit()` z powrotem w `open_video` → reguła 4 (`src/commands/video.rs:54: rule 4: admit`);
    - E1a tekst `scan_folder_service`, E2 wartość `LIBRARY_BUSY_ERROR`, E3, E4, E5 → testy use case'ów;
    - G1 glob reguły, G2 wzorzec `HOT_PATHS`, G3 glob batcha na nieistniejącą ścieżkę, G4 plik rdzenia usunięty z batchy → testy Pythona;
  - desktop E2E (prywatny Xvfb, tymczasowe `XDG_*` pod `/tmp/mediatagger-e2e09.*`, `MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"`, każdy spec wywołany jawnie, jeden naraz; katalog usunięty po przebiegu): `tauri:build:e2e` → pass; `app.smoke` 4, `app.workflows` 14, `group-order` 1, `popular-tags` 1, `review-fixes` 3, `startup-scan` 1, `about` 3, `animations` 27, `gallery-browsing` 2, `gallery-selection` 7, `gallery-timeline` 5, `grid-baseline` 3, `lightbox-activity` 14, `lightbox-images` 2, `lightbox-recovery` 2, `video-speed` 7, `search-recovery` 12, `selection-recovery` 2, `settings-language` 6, `settings-maintenance` 3, `startup-scan-remount` 2, `tag-editor` 6, `thumbnail-fade` 2, `ui.redesign` 12, `gallery.performance` 4, `gallery-100k` 3 passing; `native-video-escape` 7 passing, 1 failing („reveals idle controls when the native pointer already rests on the target”: `Native pointer can reach button[aria-label="Open asset panel"]`), dwa powtórzenia samego speca → 8 passing i 8 passing: flake środowiskowy (sprawdzenie zasięgu wskaźnika), nie regresja; 7 pozostałych testów tego speca otwiera wideo przez nowy use case;
  - `bun run tauri:dev` na prywatnym Xvfb, dwa starty pod własnym `setsid`: (1) świeży profil dev → `media.db`, `thumbs/`, `instance.lock`; (2) po dopisaniu rootu z `auto_scan_on_startup=1` (6 syntetycznych PNG) → skan startowy, 6 assetów, 6 miniatur, galeria „Tagrove” z 6 kafelkami; zamknięcie `kill -TERM` grupy, port 1420 wolny, w logach tylko SIGTERM z zamknięcia;
  - `/review-changes HEAD --save` (soczewki: conventions ×2 dzielone po katalogach, rust, tooling dodana dla skryptów Pythona i evali; raport w ignorowanym `raports/`): 0 kandydatów; jedna uwaga stylu poniżej progu (trzy puste linie przed `REPO` w `test_collect_scope.py`) poprawiona.
