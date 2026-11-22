# E15: Model lightboxa

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E08.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Port `createLightbox`: wybór, nawigacja i zapisy bez wyścigów.

## Zakres

- Każdy `select` (także zamknięcie) dostaje nowy ID żądania; spóźnione szczegóły są odrzucane.
- Nawigacja z zawijaniem po globalnym indeksie i `total`; szybkie naciśnięcia się sumują; porażka strony cofa nawigację i oferuje ponowienie.
- Prefetch sąsiadów; `getAssetQueryPosition` po odświeżeniu.
- Zapis tagów tylko przez ten model: jeden naraz, edycje scalane. Ulubione, grupa mediów.
- Usuwanie: szybka odmowa podczas skanu (ADR 0022), potwierdzenie wpisanym słowem.
- `createLightboxPresentation`: panel boczny, wygaszanie kontrolek po 3 s.

## Poza zakresem

- Widoki (E16).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/lightbox/createLightbox.ts`
- `src/features/lightbox/createLightboxPresentation.ts`
- `src/features/lightbox/types.ts`
- `src/features/lightbox/parseMediaGroupOrder.ts`

Testy do przeniesienia:
- `src/features/lightbox/__tests__`

Dokumenty i reguły:
- `docs/subsystems/lightbox.md`
- `.claude/rules/lightbox-video.md`
- `docs/adr/0022-lightbox-delete-fails-fast-during-a-scan.md`
- `docs/adr/0014-single-asset-file-mutations-guarded-by-row-cas.md`

## Kryteria akceptacji

- Testy `createLightbox` przeniesione; pominięte z powodem.
- Testy wyścigów na deterministycznym wykonawcy (select → select, select → close, zapis → nawigacja).
- Testy usuwania tylko na świeżych katalogach tymczasowych.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E15 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`

## Scenariusze porównania

- brak (epik bez widocznego odpowiednika w Tauri albo bez UI)

## Ryzyka

- Usuwanie plików: wyłącznie przez use case rdzenia.

## Status

done 2026-11-22

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E07

- Z epiku E07: lightbox czyta przez wejścia modelu zapytań z biletem (`library::Ticket`): `QueryInput::ReadAsset { ticket, index }` (`getAssetAtAsync`), `QueryInput::RetryAsset { ticket, index }` (`retryAssetAt`) i `QueryInput::ResolvePosition { ticket, asset_id }` (`getAssetPosition`; bez sesji `QueryError::Unavailable`). Odpowiedzi to `QueryEvent::AssetRead` i `QueryEvent::PositionResolved` z `AppRuntime::take_query_events()`, dokładnie jedna na bilet; odczyt wyprzedzony przez refresh/reset dostaje ją od razu (brak wiersza albo `Stale`). Synchroniczny stan nawigacji: `query.navigation_snapshot()` (`epoch`, `pending`, `total`); indeks zasobu z cache: `query.asset_index(id)` (wiersze zastępcze nie mają indeksu). Model osadź w `AppRuntime` obok zapytania (ADR 0040).

### 2026-11-22 — implementer E14

- Z epiku E14: bilety poleceń biblioteki (`LibraryInput::LoadDetails`, `Mutate`, `WaitForIdle`) wybieraj poniżej `bulk::BULK_TICKET_BASE` (2^61); zakres 2^61..2^62 należy do modelu bulk, a runtime oddaje mu odpowiedzi z takim biletem i nikomu innemu (ADR 0044). `AppRuntime::take_library_events()` zwraca teraz listę, którą runtime napełnia po każdym wejściu (bez odpowiedzi modelu bulk, z każdym `LibraryEvent::Committed`). Encja GPUI `Runtime::publish` na razie ją opróżnia i wyrzuca, bo nikt jej nie czyta: model lightboxa osadzony w `AppRuntime` powinien dostawać te zdarzenia w pętli runtime (jak `drive_bulk`), a wtedy trzeba usunąć to opróżnianie z `media_tagger_gpui/src/runtime.rs`.
- Z epiku E14: `parseMediaGroupOrder` jest już przeniesione: `bulk::parse_media_group_order` (`GroupOrderDraft::{Clear, Order, Invalid}`); normalizacja klucza grupy: `bulk::normalize_group_key`. Odczyt zegara tagów bez tworzenia rekordu: `AssetTagState::peek_generation`.

### 2026-11-22 — poprawki E14 (audyt orkiestratora)

- Z epiku E14: do przeniesienia w tym epiku została połowa przypadku `createBulk.test.ts` „ignores details completed after deletion and clears selection on identity reset”: zasób zostaje usunięty (w Solid `library.tags.remove(id)` po udanym usunięciu w lightboxie), gdy odczyt jego szczegółów dla doku jeszcze trwa. Oczekiwane: spóźniona odpowiedź nie publikuje tagów usuniętego zasobu (`single_asset_tags` puste), a `tag_details_failed` jest prawdziwe. Runtime nie ma dziś wejścia, które usuwa zasób, więc E14 przeniosło tylko połowę o resecie tożsamości (`clears_the_selection_and_the_single_tags_on_identity_reset` w `src-gpui/crates/media_tagger_client/tests/bulk_ported.rs`). Epik, który doda usuwanie, dopisuje obok drugą połowę na modelu bulk: `Fx::new` z jednym zaznaczonym zasobem i nieudanym pierwszym odczytem szczegółów, `BulkCommand::RetryTagDetails` z odczytem wstrzymanym na `ManualExecutor`, usunięcie zasobu nowym wejściem, dopiero potem odpowiedź odczytu.

### 2026-11-22 — workflow gpui-port-epic

- Zrobione: model lightboxa w `media_tagger_client::lightbox` (`model.rs`, `intents.rs`, `presentation.rs`, `delete_confirm.rs`), osadzony w `AppRuntime` (`drive_bulk` zastąpione przez `drive_models`). Każdy wybór (także zamknięcie) otwiera nowe żądanie, spóźnione szczegóły są odrzucane; nawigacja zawija się po globalnym indeksie, cofa po błędzie strony i oferuje ponowienie; zapisy tagów idą jednym torem ze scalaniem edycji, zapisy ulubionych i grup w kolejce na zasób; usuwanie tylko przez przypadek użycia rdzenia. Dodano `AssetQuery::cached_asset` i `PageCache::get`. Dokumenty: sekcja „GPUI port: the lightbox model" w `docs/subsystems/lightbox.md`, ADR 0045, uzupełnienia w system-overview, testing, frontend conventions i search-tags. Testy: 69 nowych (68 w `tests/lightbox_ported.rs`, 1 w `tests/bulk_ported.rs`; przeniesione wszystkie 47 przypadków `createLightbox.test.ts`, 6 z `presentation.test.ts`, model-owe części `MediaGroupSetter` i `LightboxModal`, druga połowa przypadku E14 o usuwaniu), plus 47 testów jednostkowych i 3 testy na prawdziwym rdzeniu (`tests/lightbox_core.rs`, tylko w `TempCore`).
- Odkryte: (1) Bilety: model lightboxa używa zakresu od `LIGHTBOX_TICKET_BASE` (2^60) do `BULK_TICKET_BASE` (2^61); jego odpowiedzi nie trafiają do `take_query_events` ani `take_library_events`; inni wywołujący muszą zostać poniżej 2^60. (2) Kolejność: model stosuje odpowiedzi bez synchronizacji, `Sync` wysyła `drive_models` po każdej partii; zapis tagów musi być zastosowany przed porównaniem z intencją edytora. (3) Zapisy tagów to `Mutation::SaveTags` z `TagEdit::Change { add, remove }`; kolejność wysyłanej listy może się różnić od Solid, a edycja w trakcie oczekiwania na innego pisarza daje drugi zapis. (4) Odczyt szczegółów rezygnuje po 8 odrzuconych odpowiedziach z rzędu (`MAX_DISCARDED_READS`), Solid ponawia bez limitu. (5) `RetryNavigation` bez celu wysyła `QueryInput::Refresh` i nie czeka. (6) Usuwanie: `LightboxCommand::DeleteSelected`, odpowiedź `DeleteSettled`; `DeleteNotice::of` daje komunikat o brakującym źródle lub oczekującym sprzątaniu. (7) `Sidebar`, `Activity`, `GroupClipboard`, `DeleteConfirm` to maszyny stanów bez timerów (`now: Duration`, `deadline()`); widok E16 musi je posiadać i planować przerysowania. (8) Porównanie słowa potwierdzenia: trim jak w JS, częściowa kompozycja NFC i małe litery Unicode, bez nowej zależności. (9) `Runtime::publish` w GPUI nadal opróżnia `take_library_events()` (zmieniony tylko komentarz), bo bez tego lista rosłaby; notatka z E14 o usunięciu opróżniania nie została wykonana. (10) Biblioteka Rust odrzuca `SaveTags` przy nieznanej liście tagów, więc test „keeps failed details and failed saves independent" odblokowuje edytor przez `MergeTags`. (11) Pominięte przypadki testowe (widok, E16): montowanie, animacje i wyjście `LightboxModal`, fokus, Escape, aria, skróty, `keyboard.test.tsx` (7 przypadków), `imageControls.test.tsx`, edytor tagów (E13/E16), element obrazu i tożsamość węzła GIF, atrybuty wejścia i przycisków w `MediaGroupSetter`, połowa `defaultPrevented` w presentation (Rust nie przyjmuje zdarzenia widoku), preloaded nieprawidłowe łańcuchy (z f64 nie da się ich zbudować; ported nieprawidłowe liczby 0, -1, 1.5, 2^53). (12) Odmowa usunięcia przy trwającym skanie na prawdziwym rdzeniu (ADR 0022) nie ma testu, bo `TempCore` nie pozwala wstrzymać blokady skanu; pokryta na atrapie (`i18n:common.libraryBusy`, zasób zostaje wybrany). (13) Do E16: widoki lightboxa, przekazanie `take_lightbox_events` w encji `Runtime` GPUI, hover-hold wyspy akcji, przełącznik panelu info, szkic tagu z alertem walidacji, skróty, `formatBytes`/`formatDuration`, porty `LightboxModal.test.tsx`, `keyboard.test.tsx`, `imageControls.test.tsx`, `MediaGroupSetter.test.tsx`. (14) Linia układu `media_tagger_client` w CLAUDE.md nie wspomina o `lightbox/`; agent workflow nie może edytować CLAUDE.md, wymaga to użytkownika. (15) Po zapisie grupy atrapa odświeża wiersze bez grupy, więc szkice podążają za kanonicznym wierszem.
- Zostało: nic nie blokuje; brak otwartych ustaleń przeglądu (2 rundy, czysto, 1 potwierdzone i naprawione), parytet bez rozbieżności (0 rund), bramka przeszła w 1 rundzie. Pozostają wymienione wyżej zakresy E16 oraz uwaga o CLAUDE.md; nic nie sprawdzono w działającej aplikacji, bo żaden widok jeszcze nie używa modelu.
- Testy: `cargo fmt --check` (GPUI): pass; `cargo clippy --all-targets --all-features --locked -D warnings` (GPUI): pass; `cargo test -p media_tagger_client` (811 jednostkowych plus integracyjne): pass; `cargo test -p media_tagger_gpui`: pass; `--test lightbox_ported --test bulk_ported` (68 i 79): pass; `architecture_boundaries` (33): pass; `bun run gpui:lock-parity`: pass; `git diff --check`: pass; `bun run test:all` (bramka): pass.

### 2026-11-22 — poprawki (audyt orkiestratora)

- Zrobione: (G1) `Sidebar::deadline`, `Activity::deadline` i `GroupClipboard::deadline` przyjmują teraz `now` i zwracają wyłącznie moment późniejszy niż `now`; po jego upływie zwracają `None`, także wtedy, gdy żadna metoda zmieniająca stan nie została jeszcze wywołana. Wcześniej `Activity` i `GroupClipboard` oddawały miniony termin do następnego wywołania, a `Sidebar` oddawał minione `Retained::Until`, więc widok uzbrajający timer z `deadline()` kręciłby się w kółko. Zachowanie Solid bez zmian (3 s, 200 ms / 0 ms, 1600 ms). (G2) Przekazanie do E16 dopisane w notatkach `E16-gpui-lightbox.md`. (G3) `Lightbox::dispose` odpowiada na każde oczekujące `DeleteSelected` zdarzeniem `LightboxEvent::DeleteSettled` z `Err(MutationError::Refused(TagStateError::Unavailable))`, w kolejności wysłania; polecenie `DeleteSelected` po dyspozycji dostaje tę samą odpowiedź i niczego nie wysyła. Każde polecenie usunięcia ma więc dokładnie jedną odpowiedź, jak polecenia modelu bulk. (G5) Szew testowy `TempCore::hold_scan_lock()` (tylko `test-support`, w `backend/core_backend.rs`) i test odmowy ADR 0022 na prawdziwym rdzeniu. Dokumenty: `docs/subsystems/lightbox.md`, ADR 0045 (jeszcze niezacommitowany), wiersze w `docs/development/testing.md`.
- Odkryte: (G3) w Solid obietnica `deleteSelected` to obietnica `mutations.deleteAsset` i rozstrzyga się także po `onCleanup` modelu (`src/features/lightbox/createLightbox.ts:397`, `:542`); w Rust biblioteka po dyspozycji nadal emituje `MutationSettled`, ale model lightboxa jest już wtedy wyłączony i wcześniej gubił odpowiedź (`src-gpui/crates/media_tagger_client/src/lightbox/model.rs`, `dispose`). Odpowiedź przy dyspozycji nie mówi, co backend zrobił z usunięciem, które było już w drodze; zapisane w kontrakcie `DeleteSettled`. (G4) Biblioteka publikuje zapisane tagi i odpowiada na zapis w jednej turze: `finish_asset_write` w `src-gpui/crates/media_tagger_client/src/library/library_model.rs` (publikacja listy :743, `Committed` :771, `settle_mutation` :795), wołane z `on_write_finished` w jednym `handle` (:1062). Sprawdzone mutacjami wycofanymi po próbie: dodatkowy `Sync` lightboxa przed odpowiedziami w `drive_models` psuje `lightbox::model::tests::saves_made_while_one_runs_are_sent_as_one_following_write` i `coalesces_same_turn_saves_and_serializes_across_close_and_reopen` (zapis `[cat, a, final]` zamiast `[cat, final]`); odroczenie odpowiedzi zapisu tagów do późniejszej tury w bibliotece psuje te same dwa testy oraz m.in. `library::library_model::tests::two_quick_tag_edits_of_one_asset_are_sent_one_after_the_other_and_merge`. Nowy test nie był potrzebny. (G5) `AppState::scan_lock` i `app::locks::with_scan_lock` są publiczne, więc szew nie wymagał zmian w `src-tauri/`; plik `backend/core_backend.rs` jest jedynym, który może nazywać `media_tagger_core::app`.
- Zostało: nic z listy audytu. Bez zmian zostają rzeczy przyjęte wcześniej: przypadki widoku dla E16, odstępstwa z ADR 0045, linia układu w CLAUDE.md, opróżnianie `take_library_events` w `Runtime::publish`. Niesprawdzone: `cargo test -p media_tagger_gpui` (crate kompiluje się pod clippy `--all-targets`; nie używa jeszcze modelu lightboxa) i `bun run test:all` (agent poprawek go nie uruchamia).
- Testy: `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --lib lightbox` → pass (52); `--test lightbox_core --test lightbox_ported` → pass (4 i 68); `job.py start E15 checks` → pass (wszystkie 7 kroków; `architecture_boundaries` 33, klient 816 jednostkowych plus integracyjne); `git diff --check` → pass. Nowe testy: `the_controls_have_no_deadline_once_the_countdown_ran_out`, `the_panel_has_no_deadline_once_its_column_is_given_back`, `the_copy_button_has_no_deadline_once_the_confirmation_ended` (`lightbox/presentation.rs`), `a_disposed_model_answers_the_deletion_that_was_on_its_way_exactly_once`, `a_disposed_model_answers_queued_and_later_deletions_and_sends_nothing` (`lightbox/model/tests.rs`), `a_deletion_during_a_scan_is_refused_as_busy_and_changes_nothing` (`tests/lightbox_core.rs`).

### 2026-11-22 — orkiestrator
- Zrobione: audyt wyniku workflow na podstawie dziennika agentów: zakres 25 z 25 plików w obu rundach przeglądu, każda z pięciu soczewek zwróciła wynik w obu rundach, żadne znalezisko nie odpadło przez pewność poniżej 60. Niesprawdzone uwagi soczewek zweryfikował i naprawił agent (wpis wyżej); po jego zmianach `job.py start E15 checks` i `job.py start E15 test-all` przeszły.
- Odkryte: sprostowanie wpisu workflow. „Parytet bez rozbieżności (0 rund)” znaczy tylko tyle, że epik nie ma scenariuszy porównania; niczego nie porównywano. Punkt (12) jest nieaktualny: odmowa usunięcia podczas skanu ma już test na prawdziwym rdzeniu (`TempCore::hold_scan_lock`, `tests/lightbox_core.rs`). Lista dla E16 z punktu (13) jest teraz także w pliku E16.
- Zostało: zakres widoków (E16); linia układu `media_tagger_client` w CLAUDE.md bez `lightbox/` (czeka na użytkownika). Nic nie sprawdzono w działającej aplikacji, bo żaden widok nie używa jeszcze modelu.
- Testy: `job.py start E15 checks` → pass (7 kroków); `job.py start E15 test-all` → pass (`[E2E] COMPLETE PASS`, exit 0)
