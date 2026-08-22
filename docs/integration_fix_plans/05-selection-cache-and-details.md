# Etap 05 - Selekcja, cache i granica summary/details

## Metryka

- Priorytet: wysoki
- Status: Ukończony
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

- [x] Utrzymać jawne typy `AssetSummary` i `AssetDetails` bez fabrykowania brakujących pól.
- [x] Wymagać details przed operacją potrzebującą ścieżki, pełnych tagów lub rozmiaru.
- [x] Dodać cache epoch powiązany z generacją bazy i query session.
- [x] Czyścić summary, details, selection i pending requests po restore lub zmianie identity epoch.
- [x] Przechowywać wybrane ID niezależnie od cache stron.
- [x] Przechowywać anchor jako globalny indeks lub rozwiązywać go przez sesję zapytania.
- [x] Budować Shift-range przez globalne indeksy i `getAssetAtAsync` albo dedykowany endpoint ID-range.
- [x] Nie uzależniać poprawności zakresu od kolejności wpisów w `Map`.
- [x] Zdefiniować zachowanie zaznaczenia, gdy rekord wypada z aktywnego filtra.
- [x] Cofnąć navigation target po rejection lub niedostępnym rekordzie.
- [x] Przeliczać albo unieważniać indeks lightboxa po zmianie sesji.
- [x] Ograniczyć i wersjonować details cache.

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

### 2026-08-22 — Etap 05 ukończony (frontend-only, bez zmian w IPC)

Zmienione pliki: `src/types.ts` (usunięto ogólny frontendowy `Asset`; dodano `SelectedAsset`, a końcowy audyt nazwał pełny wiersz legacy `LegacyAsset`), `src/hooks/useLibraryAssets.ts` (surowe `AssetSummary` w cache, `queryEpoch`, `getIdsRangeAsync`), `src/hooks/useSelectionState.ts` (widok `SelectedAsset`, LRU-256 details cache z epoką i czyszczeniem na nową sesję, cofanie navigation target, przeliczanie indeksu po zmianie sesji), `src/components/app/hooks/useBulkSelectionController.ts` (zaznaczenie po ID przeżywa eviction, anchor jako globalny indeks, Shift-range przez `getIdsRangeAsync`, LRU details cache), `src/components/app/services/assetMutationService.ts` (usunięto `updateAssetTags` — tagi nie żyją w cache galerii), warstwy gallery/lightbox/bulk przeniesione na `AssetSummary`/`SelectedAsset`, media stage wymaga details przed źródłem mediów (spinner / komunikat błędu zamiast wymyślonego źródła), info panel pokazuje `-` dla nieznanego rozmiaru. Usunięto martwe `selectNext/selectPreviousLightboxAssetAction` i `mergeBulkTagsInAssets` wraz z testami. Shift-range realizuje stronicowany `getIdsRangeAsync` nad istniejącą sesją (sekwencyjne strony po offsetach, odrzucenie całego zakresu przy błędzie strony).

Decyzje projektowe:
- Rekord wypadający z aktywnego filtra pozostaje zaznaczony (ID-y nie są przycinane do widocznych); operacje masowe działają na przecięciu z wczytanymi podsumowaniami.
- Anchor Shift-zakresu to wyłącznie zapisany globalny indeks; po restarcie sesji jest unieważniany (Shift bez kotwicy = zwykłe zaznaczenie), nigdy nie zgadujemy indeksu ze zbitej tablicy cache.
- Ścieżka startowa lightboxa: GIF/video startują z `preview_path`, obrazy czekają na details (koniec ze ścieżką fabrykowaną z `file_name`).
- Nowa sesja (`queryEpoch`) czyści oba details cache i przelicza indeks lightboxa przez `getAssetIndex`; brak pozycji = nawigacja relacyjna uśpiona do ponownego wyboru.

Testy: frontend 63 pliki / 335 testów zielonych (nowe: surowe summaries, queryEpoch, getIdsRangeAsync happy/failure, globalny Shift-range, selection vs eviction, reset anchora po epoch, cofanie nawigacji, rekomputacja indeksu, re-fetch details po nowej sesji).
