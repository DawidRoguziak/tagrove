# Etap 07 - CSV, tagi i atomowość revision

## Metryka

- Priorytet: wysoki
- Status: Ukończony
- Zależności: Etapy 03 i 04
- Następny etap: 08

## Cel

Zachować celową identyfikację CSV po basename, ale wyeliminować częściowe importy oraz ustanowić jeden invariant tagów dla UI, wyszukiwania, bazy i eksportu.

## Nienegocjowalna semantyka

- CSV zapisuje `file_name`, nie pełną ścieżkę ani fingerprint.
- Import dopasowuje po nazwie pliku.
- Jeden rekord CSV może aktualizować wiele zasobów o tej samej nazwie.

## Potwierdzone ryzyka

- Import wykonuje mutacje rekord po rekordzie i bumpuje revision dopiero na końcu.
- Błąd późnego wiersza może pozostawić wcześniejsze zmiany bez revision.
- Brakujące nagłówki `file_name` i `tags` powodują fallback do kolumn 0/1.
- Tagi z whitespace są dopuszczane przez część backendu, ale eksport/import i legacy query rozbijają je na kilka tagów.
- Favorite i group są zapisywane osobno od transakcji tagów.
- Normalizacja Unicode różni się między Rust i SQLite `lower()`.

## Zadania

- [x] Wymagać jednoznacznych, niezdublowanych nagłówków formatu CSV.
- [x] Sparsować i zwalidować cały dokument przed pierwszą mutacją.
- [x] Zachować basename-only i jawnie pokazać fan-out w podsumowaniu.
- [x] Wykonać tagi, favorite i group w jednej transakcji all-or-nothing.
- [x] Bumpować revision w tej samej transakcji.
- [x] Przenieść workflow importu z komendy do serwisu.
- [x] Ustalić jeden invariant tagu; domyślnie odrzucać whitespace i delimitery, chyba że zatwierdzony zostanie format escaping/quoting.
- [x] Egzekwować invariant na granicy backendu niezależnie od klienta.
- [x] Współdzielić reguły walidacji z UI, bulk, search i CSV.
- [x] Ujednolicić case folding używane przez import i klucze w bazie.
- [x] Usunąć legacy `split_whitespace` albo uniemożliwić zapis wartości, których nie potrafi odczytać.
- [x] Zabezpieczyć eksport CSV przed nadpisaniem aktywnych danych i publikować go atomowo.

## Kryteria odbioru

- Błąd dowolnego wiersza nie pozostawia częściowo zaimportowanych danych.
- Revision odpowiada dokładnie zatwierdzonej transakcji.
- Basename-only i fan-out pozostają zachowane.
- Ten sam tag jest akceptowany lub odrzucany identycznie w każdym przepływie.
- Eksportowany tag przechodzi zdefiniowany round-trip albo jest wcześniej odrzucony.

## Planowana weryfikacja

- Brakujące, zdublowane i przestawione nagłówki; malformed late row; duplicate basename fan-out.
- Tagi z whitespace, delimiterami i znakami Unicode.
- Błąd DB w środku importu i sprawdzenie revision po uzyskaniu osobnej zgody.

## Dziennik

### 2026-08-22 - sesja implementacyjna

- Status po sesji: Ukończony
- Zmienione pliki: serwis CSV, adaptery IPC, warstwa bazy, walidacja tagów, kontrolery mutacji frontendu, locale, testy i dokumentacja podsystemów.
- Wdrożone zachowanie: import najpierw parsuje i waliduje cały dokument, a następnie zapisuje tagi, favorite, group i revision w jednej transakcji. Eksport publikuje plik atomowo przez sibling temp. Basename fan-out pozostaje zachowany i używa klucza Unicode lowercase.
- Decyzje i odchylenia: wymagane jest dokładnie jedno wystąpienie każdego z pięciu nagłówków formatu. Tag po trim i Unicode lowercase nie może zawierać whitespace, przecinka, średnika ani znaków kontrolnych.
- Migracje i kompatybilność: `performance_schema_version` podniesiono do 3. Migracja przebudowuje `file_name_key` i rozdziela legacy tagi zawierające delimitery.
- Utworzone lub zmienione testy: dodano przypadki nagłówków, późnego błędnego wiersza, Unicode fan-out, rollbacku po błędzie DB, ochrony istniejącego eksportu, opcjonalnych scalarów, invariantów tagów, migracji legacy znaków kontrolnych i walidacji kanonicznych tagów backupu.
- Uruchomione sprawdzenia: pełne `test:all`; 60 plików i 336 testów frontendu, 126 testów Rust oraz backendowe testy integracyjne przeszły; build zakończył się powodzeniem; desktop E2E zakończyło 13 scenariuszy w 2 plikach z exit code 0.
- Niewykonane sprawdzenia i ryzyka: brak limitu rozmiaru importowanego CSV oraz fault injection dla błędu synchronizacji katalogu po publikacji eksportu. `cargo fmt --check` z lokalnym Rust 1.98 raportuje różnice formatowania w nietkniętych plikach, dlatego nie zastosowano globalnego formatowania.
- Następny dokładny krok: rozpocząć Etap 08 od przeglądu błędów, modali i dostępności.
