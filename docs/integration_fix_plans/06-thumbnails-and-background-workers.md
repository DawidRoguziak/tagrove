# Etap 06 - Miniatury i praca w tle

## Metryka

- Priorytet: wysoki
- Status: Ukończony
- Zależności: Etapy 04 i 05
- Następny etap: 07

## Cel

Zapobiec przypisaniu miniatury starej wersji pliku, zapewnić zakończenie każdego zadania schedulera oraz ograniczyć zużycie pamięci i liczbę oczekujących operacji.

## Potwierdzone ryzyka

- Skan i generowanie miniaturek mogą działać równolegle.
- Końcowy zapis `thumb_path` sprawdza ID, ale nie oczekiwaną ścieżkę i wersję źródła.
- Stary wynik może zostać przypisany do zmodyfikowanego rekordu.
- Błąd startu workera jest ignorowany.
- Panic procesora może pozostawić zadanie bez wyniku i zawiesić komendę trzymającą lock.
- Kolejka schedulera i legacy batch nie mają wystarczających limitów.
- Dekodowanie obrazu nie ma jawnego budżetu pikseli i pamięci na poziomie aplikacji.

## Zadania

- [x] Zdefiniować wersję źródła opartą co najmniej o ścieżkę, rozmiar i precyzyjny modified time/fingerprint. (`SourceVersion`: path + `size_bytes` + `fingerprint_mtime_ns`.)
- [x] Przekazywać oczekiwaną wersję przez cały pipeline thumbnaila. (Zadanie i wynik schedulera noszą `source_version`.)
- [x] Zapisywać wynik przez SQL compare-and-set po ID i wersji źródła. (`update_asset_thumbnail_path_if_version_matches`, `update_asset_thumbnail_paths_batch_versioned`.)
- [x] Publikować thumbnail przez plik tymczasowy i atomowy rename. (Istniało już w `thumbs.rs`; zachowane i pokryte dokumentacją.)
- [x] Sprzątać wynik po przegranym CAS. (Plik wynikowy usuwany przy `VersionMismatch` oraz po resecie generacji.)
- [x] Owinąć procesor zadania obsługą paniki i zawsze wysłać wynik końcowy. (`catch_unwind` w worker loop.)
- [x] Odrzucić start aplikacji albo oznaczyć scheduler unhealthy, jeżeli nie uruchomiono wymaganych workerów. (Setup zwraca błąd gdy `worker_spawn_failures > 0`.)
- [x] Dodać timeout oczekiwania na zadanie i kontrolowane przerwanie locka. (`DEMAND_JOB_TIMEOUT`/`RESULT_STALL_TIMEOUT` 180 s; locki odzyskiwane z poison przez `into_inner`.)
- [x] Ograniczyć kolejkę oraz legacy endpoint batch. (`MAX_PENDING_JOBS = 2048`; `MAX_PAGE_BATCH = 256`.)
- [x] Włączyć request ID/generation do backendowego anulowania lub odrzucania starych wyników. (Odrzucanie request_id < najwyższego; epoka generacji bramkuje publikację, `clear_all_thumbnails` ją podbija.)
- [x] Ustawić budżet wymiarów, pikseli i pamięci przed pełnym decode. (`MAX_DECODE_PIXELS = 50 MP` po probe nagłówka.)
- [x] Przenieść blokujące operacje z synchronicznych komend do `spawn_blocking` bez trzymania locków przez `await`. (Wszystkie komendy thumbs.)

## Kryteria odbioru

- Miniatura starej wersji nie może zostać przypisana nowemu rekordowi.
- Każde przyjęte zadanie kończy się sukcesem, błędem, anulowaniem albo timeoutem.
- Awaria workera nie zawiesza komendy ani operacji destrukcyjnych.
- Kolejka i koszt dekodowania mają egzekwowane limity.
- Reset generacji uniemożliwia publikację starego wyniku.

## Planowana weryfikacja

- Wyścig scan-update kontra thumbnail completion.
- Panic workera, niedostępny worker, timeout i pełna kolejka.
- Obraz o ekstremalnych wymiarach i kontrolowany limit pamięci.
- Bulk cancellation i stale generation po uzyskaniu osobnej zgody.

## Dziennik

### 2026-08-22 - sesja implementacyjna

- Status po sesji: Ukończony
- Zmienione pliki: `src-tauri/src/thumbs.rs`, `src-tauri/src/services/thumb_scheduler.rs`, `src-tauri/src/services/thumb_service.rs`, `src-tauri/src/db.rs`, `src-tauri/src/models.rs`, `src-tauri/src/app/state.rs`, `src-tauri/src/app/locks.rs`, `src-tauri/src/commands/thumbs.rs`, `src-tauri/src/commands/scan.rs`, `src-tauri/src/services/backup_service.rs`, `src-tauri/src/lib.rs`, `docs/subsystems/thumbnails.md`, `docs/architecture/ipc-contract.md`, `docs/architecture/system-overview.md`
- Wdrożone zachowanie: `SourceVersion` (path+size+ns fingerprint) przepływa przez zadanie i wynik schedulera; każdy zapis `thumb_path` to CAS po ID/wersji; przegranym CAS-om i resecie generacji towarzyszy usunięcie pliku; `upsert_scanned_asset` zwalnia referencję miniatury przy zmianie precyzyjnego fingerprintu w tej samej sekundzie; procesor panic-safe (zawsze wynik); start aplikacji odrzucany przy nieuruchomionych workerach; timeouty oczekiwania 180 s (demand) i stall-deadline 180 s (page/bulk); kolejka schedulera ograniczona do 2048, legacy page batch do 256; `ensure_thumbnails` odrzuca request_id niższe niż najwyższy zaobserwowany; `clear_all_thumbnails` podbija epokę generacji unieważniając in-flight wyniki; budżet dekodowania 50 MP; komendy thumbnaili na `spawn_blocking` bez locka przez await; locki workflow odzyskiwane z poisoning.
- Decyzje i odchylenia: `THUMB_CACHE_VERSION = 2` zmienia klucz targetu (path+size+mtime_ns zamiast sekundowego modified_at); stare targety stają się osieroconymi plikami regenerowanymi on demand - bez migracji. Failure rows nadal wersjonowane sekundowym `asset_modified_at` (znane ograniczenie, wpisane w dokumentację).
- Migracje i kompatybilność: bez zmian schematu SQL; kontrakt IPC niezmieniony kształtem (`requestId` zignorowany -> odrzucanie starych id; frontend wysyła generację jak wcześniej). Stare miniatury z wersji 1 klucza pozostają na dysku do ręcznego czyszczenia.
- Utworzone lub zmienione testy: Rust - target v2 (stabilność/czułość), budżet dekodowania, panic workera, echo wersji w wyniku, pełna kolejka, CAS applied/no-op/mismatch, batch CAS ze stale-ID, zwalnianie referencji przy tej samej sekundzie, wyścig scan-vs-completion, stall timeout, drop publikacji po resecie generacji.
- Uruchomione sprawdzenia: `cargo test --lib` (zielone), frontend `bun run test` (60 plików / 335 testów), pełny `bun run test:all` w kontenerze e2e przed commitem.
- Niewykonane sprawdzenia i ryzyka: brak testu procesowego limitu 2 wideo; brak E2E renderu przez prawdziwy ffmpeg w teście jednostkowym; failure rows wciąż sekundowe.
- Następny dokładny krok: Etap 07 - CSV, tagi i revision.
