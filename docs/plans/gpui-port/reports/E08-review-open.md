# E08: otwarte ustalenia przeglądu

Agent naprawczy luk (2026-11-21) po zakończonym workflow `gpui-port-epic`. Runda 1 przeglądu E08 przeszła tylko przez soczewki `review-conventions`, `review-performance` i `review-stability` (wszystkie bez ustaleń) i skończyła pętlę jako czysta; `review-rust` i `review-gpui` zostały pominięte bez zgłoszenia luki, bo krok zakresu zwrócił pustą listę plików. Ten plik zapisuje zastępczy przebieg obu brakujących soczewek na całej zmianie E08 (`git diff 07b7aca` plus nowe pliki), według .claude/agents/review-rust.md, .claude/agents/review-gpui.md i .claude/skills/review-changes/references/signal-bar.md.

Wynik: żadne ustalenie nie przeszło progu sygnału. Nie ma otwartych ustaleń P0, P1, P2 ani P3; ten przebieg zamyka lukę soczewek rundy 1. Kodu aplikacji i testów nie zmieniono.

## Zakres przebiegu

- Źródło: src-gpui/crates/media_tagger_client/src/library/library_model.rs, tag_state.rs, details.rs, mutations.rs, vocabulary.rs, tags.rs z testami (library_model/tests.rs, tag_state/tests.rs, vocabulary/tests.rs), zmiany w src-gpui/crates/media_tagger_client/src/library.rs, library/query.rs, runtime.rs, shell.rs, backend/core_backend.rs, lib.rs oraz testy src-gpui/crates/media_tagger_client/src/runtime/tests.rs, src-gpui/crates/media_tagger_client/tests/app_runtime.rs, src-gpui/crates/media_tagger_gpui/tests/shell_ported.rs.
- Specyfikacja: src/library/metadata/createAssetTagState.ts, createDetailsCache.ts, createMutations.ts, src/library/createLibrary.ts, src/library/vocabulary/createKnownTags.ts, src/utils/media.ts:43 i :54, src-tauri/crates/media_tagger_core/src/utils/tags.rs:9, :17, :32; ADR 0001, 0033, 0037, 0041.
- Poza czytaniem: tymczasowy test losowych przeplotów modelu `Library` na `ManualExecutor` i `FakeBackend` z pamięcią tagów i ulubionych po stronie atrapy (12 000 przebiegów po 120 do 200 kroków: zapisy wszystkich rodzajów na 5 zasobach, odczyty szczegółów, odświeżenia, bariery z resetem, usunięcia, zapisy na zaznaczeniu, błędy backendu, porzucone zadania, `Dispose`, odpowiedzi dostarczane w dowolnej kolejności względem wykonania). Test był czuły na zasiane defekty (brak unieważnienia po zapisie na zaznaczeniu, brak przesunięcia generacji przy rozliczeniu, brak rozliczenia tokenów po błędzie, zmiana liczona nie od listy kanonicznej, zdjęta bramka `TagsNotLoaded`) i przeszedł na kodzie epiku. Po przebiegu został usunięty z drzewa; nie jest częścią zmiany.

## Ustalenia

| ID | Priorytet | Ustalenie | Stan |
| --- | --- | --- | --- |
| – | – | Brak potwierdzonych ustaleń soczewek `review-rust` i `review-gpui`. | – |

## Sprawdzone tropy (odrzucone)

| Trop | Wynik |
| --- | --- |
| `normalize` i `normalize_valid` w src-gpui/crates/media_tagger_client/src/backend/core_backend.rs:64 i :70 zgłaszane przez diagnostykę edytora jako nieużywane w pewnej konfiguracji cech | Odrzucone. Obie funkcje woła bezwarunkowo src-gpui/crates/media_tagger_client/src/library/tags.rs:25, :28 i :38. `cargo check --locked` dla `-p media_tagger_client` (domyślnie, `--all-targets`, `--all-targets --all-features`, `--no-default-features`) i dla `-p media_tagger_gpui` kończy się bez ostrzeżeń; `cargo clippy --all-targets --all-features -- -D warnings` przechodzi. |
| Brak `PartialEq` dla `AssetDetails` przy porównaniu `Option<Option<Arc<AssetDetails>>>` | Odrzucone. Typ zwraca tylko pomocnik testowy src-gpui/crates/media_tagger_client/src/library/library_model/tests.rs:268; każde użycie idzie przez `flatten()` albo `matches!`, bez `==`. Testy się kompilują i przechodzą. `AssetDetails` (src-tauri/crates/media_tagger_core/src/models.rs:26) nie potrzebuje nowego `derive`. |

## Sprawdzone bez ustaleń

- **Kolejka zapisów.** Jeden zapis w locie na zasób i tożsamość, zapis na zaznaczeniu sam w kolejce (src-gpui/crates/media_tagger_client/src/library/tag_state.rs:466, :486, :517, :581); stan tylko z odpowiedzi backendu, łatki tylko dla potwierdzonych ID (src-gpui/crates/media_tagger_client/src/library/mutations.rs:460, src-gpui/crates/media_tagger_client/src/library/library_model.rs:734); po błędzie tokeny rozliczone bez publikacji i tura zwolniona, następny zapis startuje z listy kanonicznej (library_model.rs:776, :779); reset, zmiana tożsamości i `Dispose` nie publikują nic, a odpowiedź nadal zwalnia turę (tag_state.rs:615, :720, :733); nagrobek przed startem zapisu zakolejkowanego za usunięciem (library_model.rs:706, :896). W losowych przeplotach żaden zapis nie został odrzucony jako `OutOfOrder` ani `TurnUsed`, a zmiana `TagEdit::Change` nigdy nie zgubiła tagu, który miał backend.
- **Kolejność odpowiedzi.** Odczyt szczegółów skrzyżowany z zapisem jest odrzucany po generacji zegara, także gdy odpowiedź zapisu dotrze pierwsza (tag_state.rs:408, :627; src-gpui/crates/media_tagger_client/src/library/tag_state/tests.rs:79); odczyt wyprzedzony nową epoką zapytania, resetem albo `Dispose` dostaje odpowiedź od razu i późna odpowiedź niczego nie zapisuje (src-gpui/crates/media_tagger_client/src/library/details.rs:182, :255); łatka skrzyżowana ze startem zapytania albo stroną wymusza ponowny odczyt (src-gpui/crates/media_tagger_client/src/library/query.rs:789, :864).
- **Edycja tagów bez pełnej listy.** Jedyna ścieżka zamiany listy to `Mutation::SaveTags`; obie postaci `TagEdit` odmawiają w chwili startu zapisu (mutations.rs:346, library_model.rs:612), także po zapisie na zaznaczeniu, po resecie i za nieudanym odczytem. `MergeTags` i zapis na zaznaczeniu tylko dodają po stronie backendu i listy nie potrzebują, jak w src/library/metadata/createMutations.ts:140.
- **Bilety.** Każdy bilet `Mutate`, `LoadDetails`, `Refresh`, `WaitForIdle` i zewnętrznego odświeżenia słownika dostaje dokładnie jedną odpowiedź, bariera najwyżej jedną (`MaintenanceReady` albo `MaintenanceRejected`), także po `Dispose`, po resetach poza barierą i po porzuconym zadaniu (src-gpui/crates/media_tagger_client/src/model.rs:252 odpowiada `WorkerFailed`). Bariera jest przyznawana dopiero przy pustej kolejce i po kolei (tag_state.rs:670, library_model.rs:986).
- **Ograniczenia.** Rekordy tagów: 512 nieaktywnych plus przypięte i trzymane (tag_state.rs:288; tag_state/tests.rs:248, :275); szczegóły: 256 (details.rs:167); tury, loty, oczekujący na słownik i odpowiedzi odroczone znikają z końcem operacji (po każdym przeplocie puste). Nic nie rośnie z rozmiarem biblioteki; słownik rośnie z liczbą różnych tagów, jak w Solid.
- **Port i warstwy (ADR 0037).** E08 nie dodaje metody portu (src-gpui/crates/media_tagger_client/src/backend.rs bez zmian), więc `fake_backend!` i backend/core_tests.rs nie wymagają dopisków. Model nie woła portu na własnym wątku: wszystkie wywołania idą przez `Dispatch::call` (library_model.rs:1074, src-gpui/crates/media_tagger_client/src/library/vocabulary.rs:312). Reguła tagów to czyste funkcje rdzenia za core_backend.rs:59, zgodnie z decyzją ADR 0041; poza tym plikiem klient nazywa tylko `media_tagger_core::models`.
- **Normalizacja tagów.** Jedna reguła (rdzeń): U+FEFF przycinany na brzegach i niedozwolony w środku, U+0085 przycinany przez rdzeń (różnica wobec JavaScript opisana w src-gpui/crates/media_tagger_client/src/library/tags.rs:10 i w ADR 0041), `to_lowercase` po obu stronach, duplikaty po normalizacji w kolejności pierwszego wystąpienia, brak limitu długości i liczby po żadnej stronie. Tag łamiący regułę wypada z listy zamiast ją odrzucać (tags.rs:22).
- **Obsługa błędów i konwersje.** Poza testami brak `unwrap`, `expect`, indeksowania i rzutowań `as` w nowych plikach; przesunięcie i `total` słownika przez `try_from` (vocabulary.rs:177, :267).
- **Adapter GPUI.** Źródła `media_tagger_gpui` nie zmieniły się (tylko jeden test); punkty soczewki o encjach, fokusie, motywie i tekstach nie mają tu powierzchni.

## Uwagi dla późniejszych epików (nie są ustaleniami)

- Odmówiona edycja tagów (`TagsNotLoaded`) bierze i oddaje zegar zasobu, więc odczyt szczegółów tego zasobu będący w drodze zostaje odrzucony i trzeba go ponowić; tak samo działa postać funkcyjna `saveTags` w Solid. Edytory (E13, E15) bramkują przez `Library::can_edit_tags` i nie wysyłają edycji wcześniej.
- `Display` dla `TagStateError` i `MutationError` (tag_state.rs:107, mutations.rs:595) to angielskie komunikaty wyjątków z Solid. Żaden widok ich nie pokazuje; widoki mapują warianty na klucze `src/i18n/locales`.

## Co zostaje otwarte

- Ustaleń P0, P1, P2, P3: brak. Spornych i niepewnych: brak. Zastanych (sprzed 07b7aca): brak.
- Nie sprawdzono w działającej aplikacji: żaden widok nie używa jeszcze modelu biblioteki.
- Poza zakresem tego przebiegu, zapisane już w notatkach epiku: wiersz układu `media_tagger_client` w CLAUDE.md nie wymienia `library/`.
