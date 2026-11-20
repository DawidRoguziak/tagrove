# E02: Port Backend, wykonawca i potok miniatur (crate klienta)

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E00.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Dać modelom UI jedno wejście do rdzenia: typowany port zamiast IPC, z deterministycznym modelem współbieżności.

## Zakres

- Trait `Backend` (port sterujący) pokrywający każdy use case, którego używa UI: sesje zapytań, odczyty, mutacje tagów/ulubionych/grup, selekcja, skan, miniatury, CSV, backup/restore, duplikaty, usuwanie. Typy z `media_tagger_core::models`. Bez wideo i okna.
- `CoreBackend`: jedyny plik klienta sięgający do `usecases`/`services`; typowany zakres ID (dekodowanie bajtów `get_asset_query_id_range_service` w adapterze portu), błędy jako typ z rozpoznaniem prefiksu `i18n:`.
- Konwencja modeli (D4): polecenie → nowy stan + efekty; odpowiedzi wracają z tokenem żądania/generacji; timery są efektami. Trait wykonawcy z dwiema implementacjami: produkcyjna (wątki tła, w E06 podpięta pod executor GPUI) i deterministyczna do testów.
- Strumień postępu: `ProgressSink` → kanał → zdarzenia modelu; subskrypcja przed wywołaniem.
- Potok miniatur: żądanie (id, wersja, cachedOnly/prefetch/retry) przez `thumb_protocol::serve` albo `thumb_service::request_thumbnail`; bajty JPEG + wersja; anulowanie; okno „niegotowe” jak 503 w `thumb://`.
- Single-flight skanu startowego (odpowiednik `src-tauri/src/adapter/startup_scan.rs`) bez runtime'u Tauri.
- `FakeBackend` (skryptowalna kolejność odpowiedzi) i helper „prawdziwy rdzeń na tempfile” (`test-support`).

## Poza zakresem

- Modele funkcji (E07+).
- Zmiany nieaddytywne w rdzeniu.

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/platform/tauri`
- `src-tauri/src/commands`
- `src-tauri/src/adapter`
- `src-tauri/crates/media_tagger_core/src/usecases`
- `src-tauri/crates/media_tagger_core/src/services/progress.rs`
- `src-tauri/crates/media_tagger_core/src/services/thumb_protocol.rs`

Testy do przeniesienia:
- `src/platform/tauri/__tests__`

Dokumenty i reguły:
- `docs/architecture/ipc-contract.md`
- `docs/architecture/system-overview.md`
- `docs/subsystems/thumbnails.md`
- `.claude/rules/backend-core.md`
- `.claude/rules/thumbnails.md`

## Kryteria akceptacji

- Każda metoda portu ma test na prawdziwym rdzeniu z tymczasową bazą.
- Test kolejności: spóźniona odpowiedź z nieaktualnym tokenem nie zmienia stanu.
- Blokady rdzenia nigdy nie są trzymane przez granicę wykonawcy; kolejność maintenance → `scan_lock` → `thumb_lock` bez zmian (tylko przez use case'y).
- `cargo tree -p media_tagger_client -e normal` czysty; snapshot wire Tauri bez zmian.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E02 checks`:
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

- Pokusa dodania tokio albo async do rdzenia: zabronione (ADR 0034).

## Status

done 2026-11-20

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-20 — implementer E00

- Z epiku E00: zwolnienie `Boot` nie zatrzymuje wątków `ThumbnailScheduler` (rdzeń nie ma zamknięcia schedulera; wątki kończą się z procesem), więc render w toku może pisać do `thumbs/` po oddaniu blokady instancji. Jeśli potok miniatur tego epiku wymaga uporządkowanego zamknięcia, jest to addytywna zmiana rdzenia do nazwania w pliku epiku.
- Z epiku E00: `media_tagger_gpui::boot::start` zwraca `Boot { state: AppState, startup_scan_roots, .. }`; z tego powstaje `CoreBackend`. Jedyny plik klienta, który może importować `media_tagger_core::{usecases, services, app, …}`, musi się nazywać `backend/core_backend.rs` (stała `CLIENT_CORE_BACKEND` w `src-tauri/tests/architecture_boundaries.rs`); reszta klienta: tylko `models` i `error`, a alias, glob albo gołe `use media_tagger_core` jest naruszeniem.
- Z epiku E00: `[workspace.dependencies]` w `src-gpui/Cargo.toml` ma już `serde_json` i `tempfile` (wersje jak w locku Tauri). Każda nowa zależność rdzenia albo klienta, która trafia do domknięcia rdzenia, musi mieć w `src-gpui/Cargo.lock` wersję z `src-tauri/Cargo.lock` (`bun run gpui:lock-parity`).

### 2026-11-20 — workflow gpui-port-epic

- Zrobione: crate `media_tagger_client` ma port `Backend` (36 blokujących metod nazwanych jak komendy Tauri, plus `request_thumbnail` z odpowiedzią przez responder i `subscribe_progress`; bez wideo i okna) oraz `CoreBackend` (`backend/core_backend.rs`, jedyny plik z importem use case'ów i usług rdzenia; błędy jako `BackendError`). Konwencja modeli D4: `Model`/`Runner`/`Driver`/`Inbox`/`RequestToken`/`RequestSlot`/`Generation` w `model.rs`; wykonawcy `ThreadExecutor` (do 64 wątków, jeden wątek timerów) i `ManualExecutor` (deterministyczny, wirtualny zegar). Dalej: `ProgressHub`, `StartupScan`, `ThumbnailPipeline` + `ThumbnailFetcher`, a za cechą `test-support` `FakeBackend`, `ManualExecutor` i `TempCore`. Dodano 11 testów zachowania w `tests/port_contract.rs` (12 przypadków z api.test.ts ma pokrycie). Docs: ADR 0037 z wpisem w indeksie, `system-overview.md`, `ipc-contract.md`, `thumbnails.md`, `testing.md`, `setup-and-build.md`. Zmiana rdzenia tylko addytywna: derive `PartialEq, Eq` w `models.rs`.
- Odkryte: port jest synchroniczny (bez async i tokio, ADR 0037); wywołania idą przez `Dispatch::call` na `Executor`. Wiring w E06: `CoreBackend::new(Arc<AppState>, startup_scan_roots)` albo `starting()` + `attach(..)`, `ThreadExecutor` i wake przez `Driver::with_wake`. Wątki wykonawcy i publikujące miniatury nie są łączone przy drop; właściciel `AppState` i blokady instancji (E06) decyduje, jak długo czekać (`shutdown`, `wait_until_quiet`). Miniatury idą przez `thumb_service::request_thumbnail` + `finish_thumbnail_requests` z publisherem na backend, nie przez `thumb_protocol::serve`; zmiana zasad `thumb://` musi być odbita w `CoreBackend::request_thumbnail`. Odpowiedzi: `Ready`, `NotFound`, `Unavailable`, `BadRequest`; `ThumbnailPipeline` ponawia `NotFound` i `Unavailable` (500 ms, 2 s, 4 s do 30 s). Nie asertować faz postępu `scanning` i `counting` (rdzeń emituje je warunkowo); `cleanup` jest raz na root. `TempCore` nie ma ffmpeg, więc miniatury wideo nie są pokryte. Nowa komenda dla GPUI wymaga metody portu, implementacji w `CoreBackend`, linii w makrze `fake_backend!` i testu w `backend/core_tests.rs`. `Cargo.lock` w src-gpui zyskał dwie krawędzie zależności na pakiecie klienta (`tempfile` i self dev-dependency), bez nowego pakietu. Pominięte przypadki testowe: convertFileSrc (mechanika webview), natywne wideo (poza zakresem E02), limit pojemności ID (nieosiągalny przez port; testuje go rdzeń), dekoder dla niebinarnego wejścia (typowanie Rust), wyścig sekcji ADR 0027 (niedeterministyczny przez port), kształt argumentów camelCase (serializacja IPC nie istnieje w porcie). Odroczone: `rename_asset_file` poza portem (adapter Tauri go nie rejestruje); brak limitu współbieżnych pobrań miniatur i liczenia referencji dla wielu posiadaczy klucza (E09); fail-fast usuwania w trakcie skanu nietestowane przez port w `core_tests`, ale pokryte w `port_contract.rs` i testach use case'ów rdzenia.
- Zostało: nic nie blokuje; przegląd czysty po 2 rundach (1 potwierdzone i naprawione, brak otwartych), bramka przeszła w 1 rundzie. Do zrobienia przez użytkownika: zaktualizować `CLAUDE.md` (linia układu dla `media_tagger_client` mówi jeszcze „later, backend/core_backend.rs”) i `.claude/rules/gpui-port.md`; agent nie może zmieniać tych plików. `media_tagger_gpui` nie jest podpięty do backendu (E06).
- Testy:
  - `cargo test -p media_tagger_client --locked` (141 unit + 2 + 11 port_contract): pass
  - `cargo test -p media_tagger_gpui --locked` (52): pass
  - `cargo clippy --all-targets --all-features -D warnings` i `cargo fmt --check` (src-gpui): pass
  - `bun run rust:fmt`, `bun run rust:clippy`: pass
  - `architecture_boundaries` (33, wire snapshot bez zmian): pass
  - `bun run gpui:lock-parity`, `bun run test:gpui-tools`, `bun run audit:gpui`: pass
  - `bun run test:all`: pass (bramka, 1 runda; nie uruchamiane przez implementera)
