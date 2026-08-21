# Etap 06 - Miniatury i praca w tle

## Metryka

- Priorytet: wysoki
- Status: Oczekuje
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

- [ ] Zdefiniować wersję źródła opartą co najmniej o ścieżkę, rozmiar i precyzyjny modified time/fingerprint.
- [ ] Przekazywać oczekiwaną wersję przez cały pipeline thumbnaila.
- [ ] Zapisywać wynik przez SQL compare-and-set po ID i wersji źródła.
- [ ] Publikować thumbnail przez plik tymczasowy i atomowy rename.
- [ ] Sprzątać wynik po przegranym CAS.
- [ ] Owinąć procesor zadania obsługą paniki i zawsze wysłać wynik końcowy.
- [ ] Odrzucić start aplikacji albo oznaczyć scheduler unhealthy, jeżeli nie uruchomiono wymaganych workerów.
- [ ] Dodać timeout oczekiwania na zadanie i kontrolowane przerwanie locka.
- [ ] Ograniczyć kolejkę oraz legacy endpoint batch.
- [ ] Włączyć request ID/generation do backendowego anulowania lub odrzucania starych wyników.
- [ ] Ustawić budżet wymiarów, pikseli i pamięci przed pełnym decode.
- [ ] Przenieść blokujące operacje z synchronicznych komend do `spawn_blocking` bez trzymania locków przez `await`.

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

- Brak wpisów.
