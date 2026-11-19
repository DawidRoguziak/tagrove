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

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
