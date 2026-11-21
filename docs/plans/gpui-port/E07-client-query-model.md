# E07: Model zapytań i cache stron

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E02.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Port `createAssetQuery`: sesje zapytań i cache stron dla wyników do 1 000 000 pozycji.

## Zakres

- Generacja i owner ID dla każdego zapytania; przyjmowana tylko odpowiedź `ready` bieżącej generacji.
- `stale` → odświeżenie po 250 ms, nie częściej niż raz na sekundę, z `anchorOffset` = pierwszy widoczny indeks.
- Strony po 128, cache LRU na 24 strony, licznik `version` przy każdej zmianie cache.
- Sekcje osi czasu, pozycja zasobu po odświeżeniu, zakresy ID, `keepAlive` dla zaznaczenia (ADR 0029), epoka porażek.
- `queryImpact`: `none|tags|all`.

## Poza zakresem

- Rysowanie siatki (E09).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/library/query`
- `src/library/createLibrary.ts`
- `src/library/metadata/queryImpact.ts`

Testy do przeniesienia:
- `src/library/query/__tests__`
- `src/library/__tests__`

Dokumenty i reguły:
- `docs/subsystems/library-query-and-gallery.md`
- `.claude/rules/gallery-query.md`
- `docs/adr/0005-binary-id-ranges-and-anchored-refresh.md`
- `docs/adr/0007-session-timeline-sections.md`
- `docs/adr/0027-unfiltered-sections-unavailable-instead-of-stale.md`
- `docs/adr/0029-range-selection-keeps-its-session-alive.md`

## Kryteria akceptacji

- Przypadki z testów TS przeniesione; lista pominiętych z powodem w notatkach.
- Testy na deterministycznym wykonawcy: odpowiedzi spóźnione, zamienione kolejnością i `stale` w trakcie ładowania strony.
- `total` jest liczbą wyników; żadna struktura nie rośnie liniowo z `total`.
- Strony przechowują `AssetSummary` dokładnie tak, jak przyszły (bez dorabiania pól szczegółów).
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E07 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`

## Scenariusze porównania

- brak (epik bez widocznego odpowiednika w Tauri albo bez UI)

## Ryzyka

- Indeksowanie tablicy cache jak globalnej listy wyników.

## Status

done 2026-11-21

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — workflow gpui-port-epic
- Zrobione: moduł `media_tagger_client::library` (`AssetQuery`, `PageCache`, `LibraryFilters`, reguła `queryImpact`) wbudowany w `AppRuntime` (`RuntimeInput::Query`, `RuntimeEffect::Query`); własny owner ID i generacje, strony po 128, LRU 24 stron z `version`, sekcje osi czasu, pozycje, zakresy ID, keepalive (ADR 0029), epoka błędu, ponowne odczyty po patchach. Usunięto `LibraryState`, `LIBRARY_OWNER_ID`, `RuntimeInput::LibraryLoaded` i `RuntimeEffect::QueryLibrary`. Dodano ADR 0040 i zaktualizowano dokumenty właścicielskie. Testy: 58 testów implementacji plus 18 testów behawioralnych (47 przypadków TS przeniesionych), w tym opóźnione i zamienione kolejnością odpowiedzi na `ManualExecutor`, 1 000 000 wyników oraz przypadki na prawdziwym rdzeniu (`TempCore`).
- Odkryte: model jest osadzony w `AppRuntime`, nie w osobnym driverze; kolejne modele (E08, E11, E15) mają iść tą samą drogą (ADR 0040). Odczyty zwracające obietnice to bilety (`library::Ticket`) odpowiadane dokładnie raz jako `QueryEvent` (`take_query_events`); odczyt wyprzedzony przez refresh, reset lub dispose dostaje odpowiedź od razu, spóźniona odpowiedź jest odrzucana po tokenie. Model nie ma zegara: throttling to dwa efekty timera (250 ms debounce, 1 s cooldown). Owner ID to licznik procesowy (`gpui-asset-query-<n>`), więc drugi runtime na tym samym rdzeniu nie dostaje już `Superseded`. Viewport i filtry żyją w `AssetQuery` (`SetViewport`, `ApplyFilters`); brak callbacku onQueryChanged, modele per zapytanie porównują `query_epoch()`. `assets()` w kolejności indeksu. Błędy to enum `QueryError`. `patchAssets` to dane (`AssetPatch`, `SummaryPatch`: is_favorite, media_group_key, media_group_order; E08 może rozszerzyć). Rozmiar strony ograniczony do 1..=256. Popyt na strony nie jest ograniczany przez runner: E09 musi przekazywać widoczny zakres z overscanem, nigdy całego wyniku. Zakresy stron jako przedziały offsetów. Pominięte przypadki testów: createLibrary "clears assets and tags on library reset" (część tagów, słownik w E08); "rereads delayed summaries across every metadata patch" (część IPC mutations, E08); mechanika reaktywna Solid (zastąpiona obserwowalnym zachowaniem). Do E08: reszta createLibrary (refreshLibrary, handleLibraryCleared, handleImportDbRestored, resetIdentity, runMaintenance). Dopisano notatki "Z epiku E07" w E08, E09, E11, E15. Rozbieżności parity: brak.
- Zostało: brak blokad ani nierozwiązanych błędów, przeglądy bez otwartych ustaleń (2 rundy, 1 potwierdzone i naprawione), bramka przeszła. Żaden widok jeszcze nie wysyła `SetViewport`/`RetryLoad` ani nie rysuje wierszy i banera błędu (E09); brak konsumentów biletowanych odczytów (E11, E15); pełny port zestawów TS zostaje dla kroku portowania testów; wiersz układu `media_tagger_client` w CLAUDE.md nie wymienia `library/`.
- Testy:
  - `cargo fmt --check` (src-gpui): pass
  - `cargo clippy --all-targets --all-features --locked -D warnings` (src-gpui): pass
  - `cargo test --locked -p media_tagger_client`: pass (444 testów jednostkowych plus integracyjne)
  - `cargo test --locked -p media_tagger_gpui`: pass (uruchomiony przed ostatnimi drobnymi zmianami)
  - `architecture_boundaries`: pass
  - `bun run gpui:lock-parity`: pass
  - checks workflow: pass (0 poprawek)
  - `bun run test:all` (bramka): pass

### 2026-11-21 — agent naprawczy luk
- Zrobione: zamknięte ustalenie conv-2 (P3, `review-conventions`, runda 1), które nie trafiło do weryfikatora ani do naprawy: wiersz `media_tagger_client::library` w docs/development/testing.md:215 mówił o jednym przypadku na `ThreadExecutor` z prawdziwym rdzeniem, a źródło ma trzy (src-gpui/crates/media_tagger_client/src/library/query/tests.rs:1553, :2113, :2182); wiersz wymienia teraz wszystkie trzy i to, czego dowodzą. To samo „once” poprawione w docs/subsystems/library-query-and-gallery.md:385 i :419. Wiersz `media_tagger_client::runtime` (docs/development/testing.md:205) dostał zdanie o teście osadzonego modelu zapytania (src-gpui/crates/media_tagger_client/src/runtime/tests.rs:384). Zapisany raport docs/plans/gpui-port/reports/E07-review-open.md. Kodu aplikacji i testów nie zmieniono.
- Odkryte: sprostowanie poprzedniego wpisu. Zdanie „pełny port zestawów TS zostaje dla kroku portowania testów” w „Zostało” jest nieaktualne: krok Tests wykonał ten port (18 testów behawioralnych, 47 przypadków TS przeniesionych, trzy pominięte z powodami podanymi w tym samym wpisie pod „Odkryte”). Portu testów E07 nic nie zostało. Pozostałe twierdzenia części E07 w docs/subsystems/library-query-and-gallery.md, docs/architecture/system-overview.md i docs/frontend/architecture-and-ui-conventions.md sprawdzone ze źródłem i zgodne; przypadki na prawdziwym rdzeniu w src-gpui/crates/media_tagger_client/tests/app_runtime.rs:256 i :554 należą do wiersza E06 (docs/development/testing.md:206).
- Zostało: żadnych ustaleń P0–P2. P3 poza zakresem agenta: wiersz układu `media_tagger_client` w CLAUDE.md:64 nadal nie wymienia `library/`.
- Testy:
  - `git diff --check`: pass
  - `job.py start E07 checks` (base i client: `bun run rust:fmt`, `bun run rust:clippy`, `architecture_boundaries`, `cargo fmt --check` i `cargo clippy -D warnings` dla src-gpui, `bun run gpui:lock-parity`, `cargo test --locked -p media_tagger_client`): pass (444 testy jednostkowe plus integracyjne, w tym trzy przypadki na prawdziwym rdzeniu)
  - `bun run test:all`, uprząż porównawcza i desktop E2E: nie uruchamiano (zmiana tylko w dokumentach)
