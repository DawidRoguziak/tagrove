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

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E07

- Z epiku E07: lightbox czyta przez wejścia modelu zapytań z biletem (`library::Ticket`): `QueryInput::ReadAsset { ticket, index }` (`getAssetAtAsync`), `QueryInput::RetryAsset { ticket, index }` (`retryAssetAt`) i `QueryInput::ResolvePosition { ticket, asset_id }` (`getAssetPosition`; bez sesji `QueryError::Unavailable`). Odpowiedzi to `QueryEvent::AssetRead` i `QueryEvent::PositionResolved` z `AppRuntime::take_query_events()`, dokładnie jedna na bilet; odczyt wyprzedzony przez refresh/reset dostaje ją od razu (brak wiersza albo `Stale`). Synchroniczny stan nawigacji: `query.navigation_snapshot()` (`epoch`, `pending`, `total`); indeks zasobu z cache: `query.asset_index(id)` (wiersze zastępcze nie mają indeksu). Model osadź w `AppRuntime` obok zapytania (ADR 0040).

### 2026-11-22 — implementer E14

- Z epiku E14: bilety poleceń biblioteki (`LibraryInput::LoadDetails`, `Mutate`, `WaitForIdle`) wybieraj poniżej `bulk::BULK_TICKET_BASE` (2^61); zakres 2^61..2^62 należy do modelu bulk, a runtime oddaje mu odpowiedzi z takim biletem i nikomu innemu (ADR 0044). `AppRuntime::take_library_events()` zwraca teraz listę, którą runtime napełnia po każdym wejściu (bez odpowiedzi modelu bulk, z każdym `LibraryEvent::Committed`). Encja GPUI `Runtime::publish` na razie ją opróżnia i wyrzuca, bo nikt jej nie czyta: model lightboxa osadzony w `AppRuntime` powinien dostawać te zdarzenia w pętli runtime (jak `drive_bulk`), a wtedy trzeba usunąć to opróżnianie z `media_tagger_gpui/src/runtime.rs`.
- Z epiku E14: `parseMediaGroupOrder` jest już przeniesione: `bulk::parse_media_group_order` (`GroupOrderDraft::{Clear, Order, Invalid}`); normalizacja klucza grupy: `bulk::normalize_group_key`. Odczyt zegara tagów bez tworzenia rekordu: `AssetTagState::peek_generation`.

### 2026-11-22 — poprawki E14 (audyt orkiestratora)

- Z epiku E14: do przeniesienia w tym epiku została połowa przypadku `createBulk.test.ts` „ignores details completed after deletion and clears selection on identity reset”: zasób zostaje usunięty (w Solid `library.tags.remove(id)` po udanym usunięciu w lightboxie), gdy odczyt jego szczegółów dla doku jeszcze trwa. Oczekiwane: spóźniona odpowiedź nie publikuje tagów usuniętego zasobu (`single_asset_tags` puste), a `tag_details_failed` jest prawdziwe. Runtime nie ma dziś wejścia, które usuwa zasób, więc E14 przeniosło tylko połowę o resecie tożsamości (`clears_the_selection_and_the_single_tags_on_identity_reset` w `src-gpui/crates/media_tagger_client/tests/bulk_ported.rs`). Epik, który doda usuwanie, dopisuje obok drugą połowę na modelu bulk: `Fx::new` z jednym zaznaczonym zasobem i nieudanym pierwszym odczytem szczegółów, `BulkCommand::RetryTagDetails` z odczytem wstrzymanym na `ManualExecutor`, usunięcie zasobu nowym wejściem, dopiero potem odpowiedź odczytu.
