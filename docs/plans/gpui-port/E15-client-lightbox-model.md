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
