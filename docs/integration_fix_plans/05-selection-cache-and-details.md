# Etap 05 - Selekcja, cache i granica summary/details

## Metryka

- Priorytet: wysoki
- Status: Oczekuje
- Zależności: Etap 04
- Następny etap: 06

## Cel

Oddzielić częściowe dane galerii od pełnych szczegółów, oprzeć zaznaczenie o stabilne ID i jeden globalny porządek oraz uniezależnić selekcję od eviction cache renderowania.

## Potwierdzone ryzyka

- Summary jest konwertowane do pełnego `Asset` z wartościami sugerującymi kompletność.
- `assets` jest zwartą tablicą częściowego cache, nie pełnym wynikiem zapytania.
- Shift-zaznaczenie miesza globalny indeks płytki z lokalnym indeksem `assets.slice()`.
- Eviction strony usuwa ID z dostępnej tablicy i może wyczyścić zaznaczenie.
- Cache details może przetrwać zwykłe odświeżenie lub zmianę tożsamości bazy.
- Nieudana nawigacja pozostawia przesunięty `navigationTargetIndexRef`.
- Globalny indeks lightboxa nie jest przeliczany po zmianie filtra lub kolejności.

## Zadania

- [ ] Utrzymać jawne typy `AssetSummary` i `AssetDetails` bez fabrykowania brakujących pól.
- [ ] Wymagać details przed operacją potrzebującą ścieżki, pełnych tagów lub rozmiaru.
- [ ] Dodać cache epoch powiązany z generacją bazy i query session.
- [ ] Czyścić summary, details, selection i pending requests po restore lub zmianie identity epoch.
- [ ] Przechowywać wybrane ID niezależnie od cache stron.
- [ ] Przechowywać anchor jako globalny indeks lub rozwiązywać go przez sesję zapytania.
- [ ] Budować Shift-range przez globalne indeksy i `getAssetAtAsync` albo dedykowany endpoint ID-range.
- [ ] Nie uzależniać poprawności zakresu od kolejności wpisów w `Map`.
- [ ] Zdefiniować zachowanie zaznaczenia, gdy rekord wypada z aktywnego filtra.
- [ ] Cofnąć navigation target po rejection lub niedostępnym rekordzie.
- [ ] Przeliczać albo unieważniać indeks lightboxa po zmianie sesji.
- [ ] Ograniczyć i wersjonować details cache.

## Kryteria odbioru

- Summary nie jest przedstawiane jako pełny rekord.
- Shift-select wybiera dokładny globalny zakres przy paginacji, odpowiedziach poza kolejnością i eviction.
- Zaznaczenie nie znika tylko dlatego, że strona wypadła z cache renderowania.
- Stary detail nie może nadpisać nowszej mutacji ani danych po restore.
- Błąd nawigacji nie powoduje pomijania kolejnego zasobu.

## Planowana weryfikacja

- Rzadko załadowane strony, odpowiedzi poza kolejnością, eviction i duży Shift-range.
- Restore z ponownym użyciem tych samych ID.
- Nawigacja success, rejection, undefined i zmiana sesji po uzyskaniu osobnej zgody.

## Dziennik

- Brak wpisów.
