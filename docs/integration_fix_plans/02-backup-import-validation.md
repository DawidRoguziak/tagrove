# Etap 02 - Walidacja importu backupu

## Metryka

- Priorytet: krytyczny
- Status: Oczekuje
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

- [ ] Ustalić wersjonowany format bundle i identyfikator aplikacji/schematu.
- [ ] Wymagać zgodnego manifestu dla nowych backupów i jawnie ograniczyć obsługę legacy.
- [ ] Odrzucać duplikaty `media.db`, sidecarów, manifestu i docelowych ścieżek miniaturek.
- [ ] Ustawić limity liczby wpisów, rozmiaru wpisu, sumy rozpakowanych danych i współczynnika kompresji.
- [ ] Sprawdzić nagłówek i minimalny rozmiar SQLite przed otwarciem zapisywalnym.
- [ ] Otworzyć bazę początkowo read-only i uruchomić kontrolę integralności oraz foreign keys.
- [ ] Odrzucać przyszłą lub obcą wersję schematu zamiast automatycznie ją modyfikować.
- [ ] Zweryfikować semantykę wymaganych tabel, kolumn i typów danych przed migracją stagingu.
- [ ] Zweryfikować wszystkie `thumb_path`; zaakceptować wyłącznie ścieżki przepisane do staging/live thumbs.
- [ ] Zweryfikować `assets.path` względem zadeklarowanych rootów i wykonanych mapowań.
- [ ] W helperach kasowania kanonizować ścieżkę i wymagać przynależności do katalogu miniaturek.
- [ ] Nie dotykać aktywnej bazy ani aktywnych miniaturek przed pełnym sukcesem walidacji stagingu.

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

- Brak wpisów.
