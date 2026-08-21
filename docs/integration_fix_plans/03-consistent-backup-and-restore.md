# Etap 03 - Spójny backup i odzyskiwalny restore

## Metryka

- Priorytet: krytyczny
- Status: Oczekuje
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

- [ ] Wprowadzić procesowy maintenance lock obejmujący wszystkich użytkowników SQLite.
- [ ] Określić i udokumentować miejsce maintenance locka w kolejności względem `scan_lock` i `thumb_lock`.
- [ ] Zastąpić kopiowanie DB/WAL/SHM SQLite Backup API albo równoważnym spójnym snapshotem.
- [ ] Nie archiwizować pliku SHM jako trwałej części backupu.
- [ ] Tworzyć eksport w unikalnym pliku tymczasowym i publikować go atomowo po `flush`/`sync_all`.
- [ ] Odrzucać target kolidujący z profilem, bazą, sidecarami, miniaturkami i indeksowanymi źródłami.
- [ ] Jawnie wykluczać target z każdego przejścia katalogu.
- [ ] Zatrzymać nowe operacje DB i doprowadzić istniejące połączenia do bezpiecznego stanu przed restore.
- [ ] Objąć move-aside oraz install jednym mechanizmem kompensacji.
- [ ] Zachować poprzednią generację do końca integrity check, inicjalizacji, bumpu revision i odczytu kontrolnego.
- [ ] Zapisać journal restore umożliwiający odzyskanie po przerwaniu procesu między rename'ami.
- [ ] Uruchamiać recovery journal przy starcie przed zwykłym otwarciem profilu.
- [ ] Po sukcesie unieważnić wszystkie pule, sesje i cache tożsamości po obu stronach IPC.

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

- Brak wpisów.
