# Etap 01 - Bezpieczne usuwanie i atomowe rozwiązywanie duplikatów

## Metryka

- Priorytet: krytyczny
- Status: Oczekuje
- Zależności: Etap 00
- Następny etap: 02

## Cel

Zapewnić, że błąd operacji plikowej nie powoduje cichej utraty metadanych, a batch rozwiązywania duplikatów nie pozostawia nieopisanego pół-stanu.

## Potwierdzone ryzyka

- `delete_asset` usuwa rekord DB i bumpuje revision przed próbą usunięcia źródła.
- `removed_media_file: false` łączy brak pliku z błędem uprawnień lub I/O.
- Lightbox i resolver duplikatów ignorują podsumowanie usunięcia.
- Resolver wykonuje sekwencję niezależnych rename/delete; wcześniejsze operacje pozostają po późniejszym błędzie.
- Rename używa `exists()` przed `fs::rename`, co nie daje atomowego no-clobber.
- Błędy kompensacyjnego rename są ignorowane.

## Zadania

- [ ] Zdefiniować commit point i kolejność walidacji, operacji plikowej, transakcji DB, bumpu revision i kompensacji.
- [ ] Wprowadzić wynik rozróżniający usunięcie, brak źródła i błąd systemu plików.
- [ ] Nie usuwać metadanych po nieudanym usunięciu istniejącego pliku bez jawnego, odzyskiwalnego mechanizmu stagingu.
- [ ] Rozważyć przeniesienie pliku do kontrolowanego stagingu na tym samym filesystemie przed transakcją DB.
- [ ] Wykonać mutację DB i bump revision w jednej transakcji.
- [ ] Przenieść workflow usuwania i rename z komendy do serwisu.
- [ ] Zaprojektować jeden kontrakt batch dla duplikatów z oczekiwanymi ID, ścieżkami i wersjami rekordów.
- [ ] Walidować cały batch oraz kolizje nazw przed pierwszą zmianą.
- [ ] Wykonać batch pod wspólnymi lockami i raportować wynik każdej pozycji.
- [ ] Zabezpieczyć rename przed nadpisaniem celu i rozszerzyć walidację nazw Windows.
- [ ] Zmienić frontend tak, aby honorował wynik częściowy i nie pokazywał fałszywego sukcesu.

## Kryteria odbioru

- Błąd usunięcia istniejącego pliku nie usuwa bezpowrotnie jego metadanych.
- UI odróżnia pełny sukces, brak źródła i błąd plikowy.
- Batch duplikatów jest walidowany w całości przed mutacją.
- Niepowodzenie batcha ma kontrolowany rollback albo precyzyjny, możliwy do wznowienia wynik.
- Revision odpowiada dokładnie zatwierdzonej mutacji DB.

## Planowana weryfikacja

- Fault injection dla permission denied, missing file, rename collision, DB failure i compensation failure.
- Integracja na tymczasowym filesystemie i bazie.
- Desktop E2E reprezentatywnego batcha po uzyskaniu osobnej zgody.

## Dziennik

- Brak wpisów.
