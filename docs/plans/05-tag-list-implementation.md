# Plan implementacji logiki `tag-list`

## Summary
Zaimplementować modal `tag-list` jako niezależny edytor tagów do searcha: startuje bez preselektu z obecnego `filterInput`, ładuje tagi stronicowane z backendu, filtruje je po wpisywanym tekście, obsługuje reguły single/double click dla grup `include` i `exclude`, a `Potwierdź` nadpisuje cały search string, od razu uruchamia wyszukiwanie i zamyka modal.

Dodatkowo, po wyjściu z Plan Mode pierwszym krokiem ma być zapisanie tego planu do `docs/plans/05-tag-list-implementation.md`.

## Key Changes
- Rozszerzyć kontrakt frontend-backendu dla tagów:
  - dodać publiczny typ `TagListPage { items: string[]; total: number }`
  - zmienić API tagów na paginowane: `listTags({ query, offset, limit })`
  - backend `list_tags` ma przyjmować `query`, `offset`, `limit`, zwracać stabilnie sortowane wyniki i `total`
  - domyślny page size w modalu: `100`, z clampem backendowym np. `1..200`
- Dodać atomowe API do searcha w warstwie stanu:
  - w `useAppSearchFilters` dodać metodę w stylu `applyFilterInputAndSubmit(nextFilterInput, refresh?)`
  - ta metoda ma jednocześnie ustawić draft i applied filter, wyczyścić błąd walidacji, odpalić refresh i nie polegać na asynchronicznym `setState` + późniejszym `handleSearchSubmit`
  - `TagListSearchLauncher` i `TagListModal` mają dostać z góry: `knownTags` tylko jako fallback/initial hint, aktualny callback do zamknięcia i nowy callback do zastosowania searcha
- Zaimplementować stan i zachowanie modala:
  - lokalny `query` dla inputa filtra listy
  - dwa zbiory/mapy aktywnych tagów: `included` i `excluded`, startowo puste niezależnie od obecnego searcha
  - ładowanie pierwszej strony przy otwarciu, reset stron i selekcji po zamknięciu
  - przy zmianie `query`: reset listy, pobranie od offsetu `0`, ochrona przed starymi odpowiedziami przez `requestId`/`versionRef`
  - infinite scroll tylko wewnątrz listy tagów: list box z `overflow-y-auto`, pobieranie kolejnych stron po dojechaniu blisko dołu
  - modal jako całość ograniczony do viewportu, np. content wrapper z `max-h-[calc(100vh-2rem)]`, a scroll przeniesiony do listy
- Zaimplementować reguły przejść stanu tagu:
  - nieaktywny + single click -> dodaj do `included`
  - nieaktywny + double click -> dodaj do `excluded`
  - aktywny + single click -> usuń z aktywnej grupy
  - aktywny + double click -> przenieś do drugiej grupy
  - żeby uniknąć konfliktu `click` vs `dblclick`, obsłużyć klik przez mały arbiter z opóźnieniem ok. `200-250ms`: pierwszy klik planuje akcję single, drugi na tym samym tagu w oknie czasu anuluje single i wykonuje double
- Zbudować wynik searcha deterministycznie:
  - finalny string: wszystkie `included`, potem wszystkie `excluded` z prefiksem `-`
  - kolejność: sort case-insensitive po nazwie tagu
  - zachować display case z backendu, ale parser searcha dalej działa jak dziś
  - `Potwierdź` wywołuje nowe atomowe API searcha i zamyka modal dopiero po skutecznym zastosowaniu
- UI i i18n:
  - input 100% width nad listą
  - lista jako gęsty wrap małych button-chipów z dwoma kolorami aktywnego stanu, bez prefiksów `+/-` w UI
  - przyciski `Anuluj` i `Zastosuj` w stopce
  - dodać potrzebne klucze `tagList.*` do wszystkich locale: placeholder filtra, loading, empty, no matches, aria labels/stany

## Public Interfaces
- Nowy typ współdzielony: `TagListPage`
- Zmienione API frontendowe: `listTags` z prostego `query: string` na obiekt paginowany
- Nowy publiczny callback w logice searcha: atomowe `applyFilterInputAndSubmit(...)`
- Propsy `TagListSearchLauncher` / `TagListModal` rozszerzone tak, by dostawały `knownTags`, callback apply i nie trzymały search state lokalnie

## Verification
- Scenariusze manualne:
  - otwarcie modala nie preselektuje tagów z obecnego searcha
  - wpisanie filtra zawęża listę i resetuje infinite scroll
  - single/double click na nieaktywnym tagu trafia odpowiednio do include/exclude
  - single/double click na aktywnym tagu odpowiednio usuwa/przełącza grupę
  - `Potwierdź` nadpisuje cały `filterInput`, od razu odświeża wyniki i zamyka modal
  - `Anuluj` zamyka modal bez zmian
  - lista przewija się wewnątrz modala, a modal nie wychodzi poza viewport
  - szybka zmiana filtra nie miesza wyników z wcześniejszych requestów
  - po wyjściu z Plan Mode powstaje `docs/plans/05-tag-list-implementation.md` z tym planem
- Automatycznych testów nie planuję dopisywać ani aktualizować bez osobnej prośby, zgodnie z `AGENTS.md`.

## Assumptions
- Obecny search string ma być zastępowany w całości przez wynik z modala; nie zachowujemy wcześniejszych tagów ani meta-tokenów
- Startowy stan modala jest pusty, nawet jeśli topbar ma już wpisane tagi
- Kolor wystarczy do odróżnienia `include` vs `exclude`; bez ikon i bez tekstowych prefiksów w chipach
- Infinite scroll dotyczy tylko listy tagów; nie zmieniamy rozmiaru ani pozycji reszty topbara poza podaniem nowych propsów
