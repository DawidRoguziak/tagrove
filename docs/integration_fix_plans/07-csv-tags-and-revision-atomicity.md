# Etap 07 - CSV, tagi i atomowość revision

## Metryka

- Priorytet: wysoki
- Status: Oczekuje
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

- [ ] Wymagać jednoznacznych, niezdublowanych nagłówków formatu CSV.
- [ ] Sparsować i zwalidować cały dokument przed pierwszą mutacją.
- [ ] Zachować basename-only i jawnie pokazać fan-out w podsumowaniu.
- [ ] Wykonać tagi, favorite i group w jednej transakcji all-or-nothing.
- [ ] Bumpować revision w tej samej transakcji.
- [ ] Przenieść workflow importu z komendy do serwisu.
- [ ] Ustalić jeden invariant tagu; domyślnie odrzucać whitespace i delimitery, chyba że zatwierdzony zostanie format escaping/quoting.
- [ ] Egzekwować invariant na granicy backendu niezależnie od klienta.
- [ ] Współdzielić reguły walidacji z UI, bulk, search i CSV.
- [ ] Ujednolicić case folding używane przez import i klucze w bazie.
- [ ] Usunąć legacy `split_whitespace` albo uniemożliwić zapis wartości, których nie potrafi odczytać.
- [ ] Zabezpieczyć eksport CSV przed nadpisaniem aktywnych danych i publikować go atomowo.

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

- Brak wpisów.
