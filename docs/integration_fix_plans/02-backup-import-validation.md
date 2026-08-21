# Etap 02 - Walidacja importu backupu

## Metryka

- Priorytet: krytyczny
- Status: Ukończony
- Zależności: Etap 01
- Następny etap: 03

## Cel

Traktować bundle ZIP i zawartą bazę jako niezaufane wejście. Nie dopuścić do zastąpienia aktywnej biblioteki pustą, uszkodzoną lub obcą bazą ani do zapisania ścieżek umożliwiających późniejsze usunięcie plików poza profilem.

## Potwierdzone ryzyka

- Pusty `media.db` może zostać zainicjalizowany przez `init_schema` i zaakceptowany jako backup.
- Import nie uruchamia `quick_check`/`integrity_check` ani `foreign_key_check`.
- `format_version` i manifest nie są wymagane ani egzekwowane.
- Brak limitu liczby wpisów, rozmiaru po rozpakowaniu i współczynnika kompresji.
- Duplikaty krytycznych wpisów ZIP nie są odrzucane.
- Przy pustej liście mapowań `rewrite_staged_paths` wraca bez walidacji `assets.path` i `thumb_path`.
- Helpery miniaturek usuwają dowolną ścieżkę zapisaną w bazie.

## Zadania

- [x] Ustalić wersjonowany format bundle i identyfikator aplikacji/schematu.
- [x] Wymagać zgodnego manifestu dla nowych backupów i jawnie ograniczyć obsługę legacy.
- [x] Odrzucać duplikaty `media.db`, sidecarów, manifestu i docelowych ścieżek miniaturek.
- [x] Ustawić limity liczby wpisów, rozmiaru wpisu, sumy rozpakowanych danych i współczynnika kompresji.
- [x] Sprawdzić nagłówek i minimalny rozmiar SQLite przed otwarciem zapisywalnym.
- [x] Otworzyć bazę początkowo read-only i uruchomić kontrolę integralności oraz foreign keys.
- [x] Odrzucać przyszłą lub obcą wersję schematu zamiast automatycznie ją modyfikować.
- [x] Zweryfikować semantykę wymaganych tabel, kolumn i typów danych przed migracją stagingu.
- [x] Zweryfikować wszystkie `thumb_path`; zaakceptować wyłącznie ścieżki przepisane do staging/live thumbs.
- [x] Zweryfikować `assets.path` względem zadeklarowanych rootów i wykonanych mapowań.
- [x] W helperach kasowania kanonizować ścieżkę i wymagać przynależności do katalogu miniaturek.
- [x] Nie dotykać aktywnej bazy ani aktywnych miniaturek przed pełnym sukcesem walidacji stagingu.

## Kryteria odbioru

- Pusta, uszkodzona, obca lub przyszła baza nie zastępuje danych aplikacji.
- ZIP przekraczający limit jest odrzucany przed niekontrolowanym zużyciem zasobów.
- Żadna ścieżka z importu nie umożliwia usunięcia pliku poza dozwolonym katalogiem.
- Błąd walidacji pozostawia aktywny profil bez zmian.
- Obsługa legacy ma jawnie opisane granice kompatybilności.

## Planowana weryfikacja

- Fixtures: zero-byte DB, losowy plik, uszkodzona baza, future schema, obca baza, FK violation i duplicate ZIP entries.
- Próby path escape przez `assets.path`, `thumb_path`, symlink i różne separatory.
- Test limitów ZIP i kontrolowanego sprzątania stagingu po uzyskaniu osobnej zgody.

## Dziennik

### 2026-08-21 - sesja implementacyjna

- Status po sesji: Ukończony
- Zmienione pliki: `src-tauri/src/services/backup_service.rs`, `src-tauri/src/db.rs`, `src-tauri/src/services/thumb_service.rs`, `src-tauri/src/services/asset_mutation_service.rs`, `src-tauri/src/commands/assets.rs`, `src-tauri/src/commands/scan.rs`, `src-tauri/src/commands/import_export.rs`, `docs/architecture/ipc-contract.md`, `docs/subsystems/database.md`, `docs/subsystems/thumbnails.md`, `docs/subsystems/data-safety-and-portability.md`, `docs/integration_fix_plans/README.md`, `docs/integration_fix_plans/02-backup-import-validation.md`.
- Wdrożone zachowanie: format bundle v2 zawiera stały identyfikator aplikacji i wersję schematu; preflight całego ZIP egzekwuje limity, typy wpisów i unikalne znormalizowane cele; staging przechodzi kontrolę nagłówka SQLite, read-only `integrity_check`, `foreign_key_check`, zgodności markerów, tabel, kolumn, ograniczeń, FK, typów i danych przed migracją; import odrzuca lokalny journal operacji, waliduje ownership aktywów i symlinki, bezpiecznie przepisuje miniaturki oraz powtarza walidację po migracji; helpery kasowania usuwają tylko kanoniczne regularne pliki poniżej katalogu miniaturek.
- Decyzje i odchylenia: format v2 to jedyny format nowych eksportów; legacy ograniczono do manifestu v1 lub braku manifestu oraz markerów `0/0` z rozpoznanym schematem MediaTagger; brakujące lub niejednoznaczne referencje miniaturek wolno wyzerować tylko dla legacy, a format v2 jest odrzucany; niepusty `pending_file_operations` jest odrzucany również przy eksporcie, ponieważ journal nie jest przenośnym stanem profilu; limity wynoszą 8 GiB archiwum, 250 000 wpisów, 8 GiB na wpis, 32 GiB po rozpakowaniu, współczynnik 1000:1 i 1 MiB manifestu; mechanika swap/rollback pozostaje niezmieniona i należy do Etapu 03.
- Migracje i kompatybilność: `init_schema` nadaje istniejącej poprawnej bazie SQLite `application_id = 0x4d544147` i `user_version = 1`; przyszłe i obce niezerowe markery są odrzucane przed mutacją; zaakceptowany markerless legacy jest migrowany wyłącznie w stagingu.
- Utworzone lub zmienione testy: nie tworzono ani nie zmieniano bez osobnego polecenia.
- Uruchomione sprawdzenia: `git diff --check`; statyczny review kodu; próba `cargo fmt` zakończyła się przed uruchomieniem, ponieważ `cargo` nie jest dostępne w środowisku (`command not found`). Nie uruchamiano testów, buildów ani E2E.
- Niewykonane sprawdzenia i ryzyka: brak kompilacji, formattera oraz fixtures z sekcji Planowana weryfikacja; istniejący test udanego restore ma asset poza zadeklarowanym rootem i po uzyskaniu zgody wymaga dostosowania fixture do nowego kontraktu; odporność na TOCTOU między kanonizacją a `remove_file` pozostaje ograniczeniem operacji ścieżkowych; bundle nie ma podpisu ani szyfrowania.
- Następny dokładny krok: w nowej sesji rozpocząć wyłącznie Etap 03 od ponownego odczytu `03-consistent-backup-and-restore.md`, dokumentacji bezpieczeństwa danych i aktualnego workflow swap/rollback.
