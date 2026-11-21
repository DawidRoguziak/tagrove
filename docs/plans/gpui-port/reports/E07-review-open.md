# E07: otwarte ustalenia przeglądu

Agent naprawczy luk (2026-11-21) po zakończonym workflow `gpui-port-epic`. Pętla przeglądu skończyła się czystą rundą (runda 2), więc ten plik nie zawiera otwartych ustaleń P0–P2. Zapisuje ustalenie P3 soczewki `review-conventions` z rundy 1, które nie trafiło do weryfikatora ani do naprawy i którego runda 2 nie podniosła ponownie, oraz wynik sprawdzenia pozostałych twierdzeń o testach i modelu w dokumentach dopisanych przez E07.

## Zakres przebiegu

- Wiersz `media_tagger_client::library` w docs/development/testing.md:215 i wiersz `media_tagger_client::runtime` w docs/development/testing.md:205.
- Części E07 w docs/subsystems/library-query-and-gallery.md:312 (sekcja „GPUI port: the query model”) i :419, docs/architecture/system-overview.md:181 i :338, docs/frontend/architecture-and-ui-conventions.md:47.
- Źródło: src-gpui/crates/media_tagger_client/src/library/query.rs, library/page_cache.rs, library/filters.rs, library/query_impact.rs, library/query/tests.rs, runtime.rs, runtime/tests.rs, tests/app_runtime.rs oraz nazwy po stronie Solid w src/library/query/createAssetQuery.ts i src/library/createLibrary.ts.

## Ustalenia

| ID | Priorytet | Ustalenie | Stan |
| --- | --- | --- | --- |
| conv-2 | P3 | docs/development/testing.md:215 podawał, że jeden przypadek uruchamia model na `ThreadExecutor` z prawdziwym rdzeniem przez `TempCore`. Źródło ma trzy takie przypadki: src-gpui/crates/media_tagger_client/src/library/query/tests.rs:1553 (odczyty w tle: pierwsza strona, strony zakresu, zakres ID, pozycja, keepalive, sekcje), :2113 (sesja nieaktualna po skanie zastąpiona po debounce, nowy `total`) i :2182 (odświeżenie z kotwicą niesie widoczną stronę). To samo „once” stało w docs/subsystems/library-query-and-gallery.md:385. | potwierdzone i naprawione: docs/development/testing.md:215 wymienia trzy przypadki i to, czego dowodzą; docs/subsystems/library-query-and-gallery.md:385 i :419 mówią o trzech przypadkach na prawdziwym rdzeniu |
| gap-1 | P3 | Wiersz `media_tagger_client::runtime` (docs/development/testing.md:205) nie wymieniał testu dodanego przez E07: src-gpui/crates/media_tagger_client/src/runtime/tests.rs:384 (osadzony model zapytania przez `RuntimeInput::Query`: strona zakresu, odczyt z biletem, timer odświeżenia po `Stale`). | naprawione: dopisane jedno zdanie w docs/development/testing.md:205 |

Sprawdzone bez zmian: src-gpui/crates/media_tagger_client/tests/app_runtime.rs:256 i :554 to przypadki E06 na prawdziwym rdzeniu z własnym wierszem (docs/development/testing.md:206); E07 zmienił w nich tylko odczyt `query().total()` i `query().session_id()`, więc wiersz `library` ich nie wymienia. Pozostałe twierdzenia części E07 zgadzają się ze źródłem: stałe (128, 1–256, 24, 250 ms, 1 s, 1 000 000) w src-gpui/crates/media_tagger_client/src/library/query.rs:88 i src-gpui/crates/media_tagger_client/src/library/page_cache.rs:28, odpowiedzi na odczyty wyprzedzone przez odświeżenie (query.rs:571), kotwica także dla wstrzymanego zakresu (query.rs:619), dwa timery zamiast zegara (query.rs:1126, :1137, :1152), licznik właściciela (query.rs:101), kolejność `assets()` (page_cache.rs:205), ponowny odczyt po patchu (query.rs:789, :861), osadzenie w runtime (src-gpui/crates/media_tagger_client/src/runtime.rs:405, :466) i liczba w nagłówku z `total()` (src-gpui/crates/media_tagger_gpui/src/views/gallery.rs:209). Żaden widok nie czyta `load_error()`, jak podaje dokument.

## Co zostaje otwarte

- Ustaleń P0, P1 ani P2: brak.
- P3, poza zakresem tego przebiegu: wiersz układu `media_tagger_client` w CLAUDE.md:64 nie wymienia `library/` (zapisane już w notatkach epiku; agent naprawczy nie zmienia CLAUDE.md).
- Ustaleń E07 do naprawy w kodzie epiku: brak. Kodu aplikacji i testów ten przebieg nie zmienił.
