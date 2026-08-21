# Etap 03 - Spójny backup i odzyskiwalny restore

## Metryka

- Priorytet: krytyczny
- Status: Ukończony
- Zależności: Etap 02
- Następny etap: 04

## Cel

Zapewnić jeden spójny snapshot SQLite oraz crash-safe podmianę bazy i miniaturek, bez kolizji z równoległymi użytkownikami połączeń.

## Potwierdzone ryzyka

- Eksport kopiuje kolejno `media.db`, WAL i SHM bez gwarancji jednego punktu w czasie.
- Zwykłe mutacje metadanych mogą działać podczas eksportu i restore.
- Target eksportu może wskazać aktywną bazę, źródłowe medium albo katalog miniaturek.
- ZIP utworzony wewnątrz `thumbs/` może dołączyć sam siebie.
- Przeniesienie DB i miniaturek do `*.restore-previous` nie jest objęte jednym rollbackiem.
- Poprzednia generacja jest kasowana przed końcowym otwarciem, inicjalizacją i bumpem revision.
- Pool invalidation nie zamyka wydanych połączeń.

## Zadania

- [x] Wprowadzić procesowy maintenance lock obejmujący wszystkich użytkowników SQLite.
- [x] Określić i udokumentować miejsce maintenance locka w kolejności względem `scan_lock` i `thumb_lock`.
- [x] Zastąpić kopiowanie DB/WAL/SHM SQLite Backup API albo równoważnym spójnym snapshotem.
- [x] Nie archiwizować pliku SHM jako trwałej części backupu.
- [x] Tworzyć eksport w unikalnym pliku tymczasowym i publikować go atomowo po `flush`/`sync_all`.
- [x] Odrzucać target kolidujący z profilem, bazą, sidecarami, miniaturkami i indeksowanymi źródłami.
- [x] Jawnie wykluczać target z każdego przejścia katalogu.
- [x] Zatrzymać nowe operacje DB i doprowadzić istniejące połączenia do bezpiecznego stanu przed restore.
- [x] Objąć move-aside oraz install jednym mechanizmem kompensacji.
- [x] Zachować poprzednią generację do końca integrity check, inicjalizacji, bumpu revision i odczytu kontrolnego.
- [x] Zapisać journal restore umożliwiający odzyskanie po przerwaniu procesu między rename'ami.
- [x] Uruchamiać recovery journal przy starcie przed zwykłym otwarciem profilu.
- [x] Po sukcesie unieważnić wszystkie pule, sesje i cache tożsamości po obu stronach IPC.

## Kryteria odbioru

- Backup reprezentuje jeden spójny punkt w czasie.
- Równoległa mutacja nie może wejść podczas snapshotu ani podmiany danych.
- Awaria w dowolnym miejscu restore pozostawia kompletną poprzednią lub nową generację.
- Eksport nie może nadpisać aktywnych danych ani dołączyć sam siebie.
- Po restore stare połączenie lub cache nie odczytuje poprzedniej generacji.

## Planowana weryfikacja

- Równoległe mutacje podczas snapshotu.
- Fault injection przy każdym rename, sync, open, init, revision bump i cleanup.
- Recovery po wymuszonym przerwaniu procesu.
- Desktop E2E backup/restore po uzyskaniu osobnej zgody.

## Dziennik

### 2026-08-21 - sesja implementacyjna

- Status po sesji: Ukończony
- Zmienione pliki: `src-tauri/Cargo.toml`, `src-tauri/src/db.rs`, `src-tauri/src/app/locks.rs`, `src-tauri/src/services/db_pool.rs`, `src-tauri/src/services/media_server.rs`, `src-tauri/src/services/backup_service.rs`, `src-tauri/src/commands/import_export.rs`, `src-tauri/src/lib.rs`, `docs/architecture/system-overview.md`, `docs/architecture/ipc-contract.md`, `docs/subsystems/database.md`, `docs/subsystems/data-safety-and-portability.md`, `docs/subsystems/library-query-and-gallery.md`, `docs/subsystems/settings-operations.md`, `docs/integration_fix_plans/README.md`, `docs/integration_fix_plans/03-consistent-backup-and-restore.md`.
- Wdrożone zachowanie: wszystkie normalne połączenia SQLite mają procesową dzierżawę; maintenance zamyka bramę, unieważnia i opróżnia pulę, a następnie używa kolejności `maintenance -> scan_lock -> thumb_lock`. Eksport używa SQLite Backup API, nie zapisuje sidecarów, publikuje zsynchronizowany unikalny plik tymczasowy atomowym rename i odrzuca targety wewnątrz profilu lub indeksowanych źródeł. Restore checkpointuje staging po przepisaniu ścieżek, journaluje fazy `swapping`/`committed`, kompensuje całą podmianę i uruchamia idempotentne recovery przed zwykłym otwarciem profilu.
- Decyzje i odchylenia: format pozostaje w wersji 2; importer nadal akceptuje zwalidowane WAL/SHM dla zgodności ze starszymi kopiami, ale bieżący eksporter zapisuje wyłącznie jeden `media.db`. Frontend nie wymagał zmiany, ponieważ istniejący barrier już resetuje tagi, details, query pages i thumbnail identity przed zwolnieniem maintenance po obu wynikach komendy.
- Migracje i kompatybilność: brak migracji schematu i IPC; włączono istniejącą funkcję `backup` zależności `rusqlite`. Starsze archiwa z opcjonalnymi sidecarami pozostają obsługiwane.
- Utworzone lub zmienione testy: nie wykonywano bez osobnego polecenia.
- Uruchomione sprawdzenia: `git diff --check` zakończone pomyślnie; `cargo fmt --check` nie zostało uruchomione, ponieważ `cargo` nie jest dostępne w środowisku.
- Niewykonane sprawdzenia i ryzyka: zgodnie z poleceniem nie uruchomiono testów, buildów ani E2E; nie wykonano planowanych prób równoległości, fault injection i recovery po wymuszonym przerwaniu procesu. Istniejące fixture testów backupu nadal opisują eksport sidecarów i zapis targetu obok aktywnej bazy, więc wymagają aktualizacji po uzyskaniu osobnej zgody. Trwałość rename/sync nadal zależy od gwarancji hostowego systemu plików, a maintenance bez timeoutu może czekać na zawieszonego użytkownika DB.
- Następny dokładny krok: w nowej sesji rozpocząć wyłącznie Etap 04 od ponownego odczytu jego planu i aktualnego kodu sesji zapytań/invalidation.
