# Etap 04 - Sesje zapytań i invalidation

## Metryka

- Priorytet: wysoki
- Status: Oczekuje
- Zależności: Etap 03
- Następny etap: 05

## Cel

Zapewnić prawidłową semantykę latest-request-wins, spójny snapshot pierwszej strony oraz centralne odświeżanie membership i ordering po każdej mutacji widocznej dla galerii.

## Potwierdzone ryzyka

- Frontend przekazuje `generation`, ale backend ją ignoruje.
- Backend przydziela request ID dopiero wewnątrz pracy przekazanej do `spawn_blocking`.
- Scheduler może odwrócić kolejność rozpoczęcia dwóch żądań.
- Revision, lista ID i pierwsza strona nie są odczytywane w jednym snapshotcie.
- Zmiana tagów lub grupy patchuje lokalny obiekt bez ponownej oceny aktywnego filtra i kolejności.
- Część mutacji zapisuje dane i bumpuje revision w osobnych autocommitach.
- Błędy `refresh` i `loadPage` nie mają kontrolowanego stanu błędu i Retry.

## Zadania

- [ ] Ujednolicić request ID/generation w kontrakcie TypeScript-Rust.
- [ ] Zarejestrować kolejność żądania przed `spawn_blocking` albo oprzeć supersession na generation klienta.
- [ ] Dodać kooperacyjne sprawdzanie anulowania podczas kosztownego budowania listy ID.
- [ ] Odczytać revision, ordered IDs i pierwszą stronę z jednego snapshotu SQLite.
- [ ] Zdefiniować jednoznaczne wyniki `ready`, `superseded`, `stale` i błąd.
- [ ] Zbudować centralną ścieżkę invalidation dla scan, tag, favorite, group, rename, delete, CSV i restore.
- [ ] Po mutacji wpływającej na filtr lub ordering uruchomić nową sesję zamiast wyłącznie patchować cache.
- [ ] Połączyć każdą mutację widoczną w query z revision w tej samej transakcji.
- [ ] Dodać stan błędu, Retry i kontrolowane ponawianie stron.
- [ ] Nie oznaczać zakresu wirtualnego jako trwale obsłużony przed sukcesem strony.
- [ ] Ograniczyć czas oczekiwania na connection pool i raportować stan busy.

## Kryteria odbioru

- Starsze żądanie nie może unieważnić nowszego przez kolejność schedulera.
- Pierwsza strona i sesja odpowiadają tej samej revision i temu samemu snapshotowi.
- Każda udana mutacja powoduje prawidłowy membership i ordering aktywnego widoku.
- Błąd strony nie tworzy trwałej dziury i jest możliwy do ponowienia.
- Bump revision nie może zostać utracony po zatwierdzonej mutacji.

## Planowana weryfikacja

- Kontrolowane odwrócenie kolejności dwóch requestów.
- Mutacje tag/favorite/group przy aktywnych filtrach i ordering grupowym.
- Błąd strony, stale session, eviction i Retry.
- Test realnego IPC dla generation po uzyskaniu osobnej zgody.

## Dziennik

- Brak wpisów.
