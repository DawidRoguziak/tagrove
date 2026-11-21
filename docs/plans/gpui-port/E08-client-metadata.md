# E08: Szczegóły, stan tagów, mutacje i słownik

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E07.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Port warstwy `library/metadata` i `library/vocabulary`: kto i kiedy może pisać tagi oraz co publikujemy po odpowiedzi backendu.

## Zakres

- Cache szczegółów LRU na 256 wpisów; czyszczony przy nowym zapytaniu.
- Stan tagów: kolejka zapisów per zasób (ADR 0001), epoka tożsamości, bariera mutacji tagów dla clear/CSV/restore.
- Mutacje: ulubione, tagi (zamiana i scalanie zbiorcze), grupa mediów, usunięcie, mutacja selekcji; publikacja tylko potwierdzonych ID.
- Słownik znanych tagów (`list_tags` stronami, `vocabulary_changed`).
- Normalizacja tagów: użyć `media_tagger_core::utils::tags` zamiast drugiej implementacji; test zgodności z `src/utils/media.ts`.

## Poza zakresem

- UI edytora tagów (E13).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/library/metadata`
- `src/library/vocabulary`
- `src/utils/media.ts`
- `src-tauri/crates/media_tagger_core/src/utils/tags.rs`

Testy do przeniesienia:
- `src/library/metadata/__tests__`
- `src/library/vocabulary/__tests__`
- `src/utils/__tests__`

Dokumenty i reguły:
- `docs/subsystems/search-tags-and-media-groups.md`
- `.claude/rules/search-tags.md`
- `docs/adr/0001-per-asset-metadata-write-queue.md`
- `docs/adr/0033-feff-is-tag-whitespace-in-rust-and-legacy-rows-are-split-on-read.md`

## Kryteria akceptacji

- Dwa szybkie zapisy tego samego zasobu: jeden w locie, edycje scalone, stan z kanonicznej odpowiedzi.
- Częściowy wynik zbiorczy publikuje tylko potwierdzone ID.
- Edycja tagów zablokowana, dopóki nie jest znana pełna lista tagów zasobu.
- Przypadki z testów TS przeniesione; pominięte z powodem.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E08 checks`:
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

- Optymistyczny zapis bez rollbacku: zabroniony.

## Status

done 2026-11-21

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E07

- Z epiku E07: model zapytań to `media_tagger_client::library::AssetQuery`, osadzony w `AppRuntime` (`RuntimeInput::Query`, `RuntimeEffect::Query`, runner przez `run_query_effect(backend, effect, dispatch, wrap)`; ADR 0040). Modele tego epiku osadzaj tak samo, obok zapytania. Nie ma callbacku `onQueryChanged`: cache szczegółów czyści się, gdy zmieni się `query.query_epoch()` (porównaj po każdym wejściu przekazanym do zapytania).
- Z epiku E07: `patchAssets` to `QueryInput::PatchAssets(AssetPatch)`: `AssetPatch::Update(Vec<(id, SummaryPatch)>)` z polami `is_favorite`, `media_group_key`, `media_group_order` (bez tagów, jak w Solid) i `AssetPatch::Remove(ids)`. Nowe pole podsumowania w łatce dopisz w `library/page_cache.rs` (`SummaryPatch::apply`). Odświeżenie po mutacji to `QueryInput::Refresh`; reguły są gotowe w `library::tag_mutation_requires_refresh` i `favorite_mutation_requires_refresh` (filtry z `query.filters()`).
- Z epiku E07: `createLibrary` jest przeniesione tylko w części zapytania (filtry, zakres, kotwica). `refreshLibrary` (zapytanie + słownik), `handleLibraryCleared` (`QueryInput::Reset`, potem `QueryInput::SetViewport(None)`, reset słownika i tożsamości), `handleImportDbRestored` (`QueryInput::Reset` bez zmiany zakresu) i `runMaintenance` (bariera mutacji) należą do tego epiku; przypadek „clears assets and tags on library reset” z `createLibrary.test.ts` da się domknąć dopiero ze słownikiem.

### 2026-11-21 — workflow gpui-port-epic

- Zrobione: model `Library` (`library/library_model.rs`, port `createLibrary` + `createMutations`) osadzony w `AppRuntime` (ADR 0041); `AssetTagState` (kolejka zapisów per zasób z ADR 0001 jako jawny stan: tury, epoka tożsamości, nagrobki, piny, 512 nieaktywnych rekordów, bariera konserwacji); `DetailsCache` (LRU 256, jeden odczyt na zasób, czyszczony przy zmianie epoki zapytania); mutacje jako dane (tagi: zamiana i scalanie zbiorcze, ulubione, grupy mediów, usuwanie, trzy zapisy wg specyfikacji zaznaczenia), łatki tylko z odpowiedzi backendu; `KnownTags` (`list_tags` stronami po 200, `vocabulary_changed`, ponowienie); normalizacja tagów w `library::tags` przez `backend/core_backend.rs`. Testy: 58 testów rozwojowych plus 11 dopisanych (5 przeniesionych z TS, 6 dodatkowych do kryteriów akceptacji: częściowy wynik zbiorczy tagów i ulubionych, `TagsNotLoaded` po zapisie na całym zaznaczeniu i po resetach, dwie prośby o konserwację po kolei, nowe filtry czyszczą cache szczegółów).
- Odkryte: `RuntimeInput::Library(Box<LibraryInput>)` (buduj przez `RuntimeInput::library`), efekty `RuntimeEffect::Library`, stan przez `AppRuntime::library()`, odpowiedzi przez `take_library_events()`. `RefreshLibrary` = zapytanie + słownik, więc start kolejkuje o jedno wywołanie więcej (`list_tags`; testy liczące zadania oczekują pięciu). Zapis to `LibraryInput::Mutate { ticket, mutation }` z jedną odpowiedzią `MutationSettled`; opublikowane zmiany jako `LibraryEvent::Committed`. `TagEdit::Change { add, remove }` to forma funkcyjna `saveTags`; `TagEdit::Replace` to lista. Surowsze niż Solid: oba warianty zwracają `MutationError::TagsNotLoaded`, gdy pełna lista tagów zasobu nie jest znana; edytory (E13, E15) bramkują przez `Library::can_edit_tags(id)`. Bariera konserwacji dla clear/CSV/restore: `BeginMaintenance` -> `MaintenanceReady` -> operacja -> `EndMaintenance { reset: Identity | Restored | Cleared }` (reset także po błędzie operacji). Piny tagów (`PinTags`/`UnpinTags`) są liczone, wołający musi je zbilansować. Kolejność tagów kanonicznych jest z backendu (sortowanie po nazwie), edytory pokazują `Library::asset_tags`. Zapytanie zaczyna nową epokę przy starcie i przy odpowiedzi, więc odczyt szczegółów wysłany przed odpowiedzią zapytania jest odrzucany. Wartości `Ticket` wybiera wołający i są wspólne dla poleceń biblioteki; kolejne modele muszą rozdzielić swoje zakresy. `run_query_effect`, `run_vocabulary_effect`, `run_library_effect` biorą ogólne `wrap` zamiast wskaźnika `fn`. Różnica względem `media.ts`: rdzeń przycina U+0085 na brzegu tagu, JavaScript odrzuca taki tag. Poza zakresem (odłożone do E12/E15/E16): gramatyka wyszukiwania (`parseSearchFilter`, `readGroupNameFilterValue`, cytowane tokeny, `literalTagSearch.test.ts`, przypadki gramatyki z `media.test.ts`) do E12; `formatBytes`/`formatDuration` do panelu lightboxa (E15/E16); postać tekstowa `normalizeTags` (podział po białych znakach) nieportowana, bo nikt jej nie woła ze stringiem; test odmontowania koordynatora i „enqueueMutation nie zwraca Promise” pominięte (pokryte przez `dispose()` i `Admission.ready`). Żaden widok jeszcze nie używa modelu biblioteki. Parytet: brak otwartych różnic.
- Zostało: nic nie blokuje; brak niepowodzeń i otwartych znalezisk przeglądu (1 runda, czysto), bramka zaliczona w 1 rundzie. Linia układu `media_tagger_client` w CLAUDE.md nadal nie wymienia `library/`; agent workflow nie może edytować CLAUDE.md, wymaga to użytkownika. `DetailsCache` nie ma bezpośrednich testów jednostkowych (pokryty przez model biblioteki).
- Testy:
  - `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`: pass
  - `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`: pass
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked` (client 511 testów jednostkowych + integracyjne, `media_tagger_gpui`): pass
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`: pass (33)
  - `bun run gpui:lock-parity`: pass
  - `git diff --check`: pass
  - `test:all`: pass (bramka, 1 runda)
  - `bun run rust:fmt`, `bun run rust:clippy`, uruchomienia desktop i compare: nie uruchamiano (nic w `src-tauri/`, `src/`, `e2e/` się nie zmieniło)

### 2026-11-21 — agent naprawczy luk (review-rust, review-gpui)

- Zrobione: runda 1 przeglądu E08 przeszła bez soczewek `review-rust` i `review-gpui` (krok zakresu zwrócił pustą listę plików, workflow pominął obie bez zgłoszenia luki). Zastępczy przebieg obu soczewek na całej zmianie E08 względem źródeł Solid, reguły rdzenia i ADR 0001, 0033, 0037, 0041; wynik w [reports/E08-review-open.md](reports/E08-review-open.md). Nic nie naprawiano: żadne ustalenie nie przeszło progu sygnału, kod i testy epiku bez zmian.
- Odkryte: dwa tropy z diagnostyki edytora odrzucone: `tag_rules::normalize` i `normalize_valid` są używane bezwarunkowo (src-gpui/crates/media_tagger_client/src/backend/core_backend.rs:64, src-gpui/crates/media_tagger_client/src/library/tags.rs:25), `cargo check` klienta i `media_tagger_gpui` bez ostrzeżeń w każdej konfiguracji cech; `Option<Option<Arc<AssetDetails>>>` nigdzie nie jest porównywane przez `==` (src-gpui/crates/media_tagger_client/src/library/library_model/tests.rs:268). Tymczasowy test losowych przeplotów (12 000 przebiegów, odpowiedzi w dowolnej kolejności, błędy, bariery, resety, `Dispose`) potwierdził: jeden zapis w locie na zasób, każdy bilet z jedną odpowiedzią, stan zgodny z backendem po opróżnieniu kolejki, zmiana `TagEdit::Change` liczona od listy kanonicznej, nic nie zostaje po zakończeniu; usunięty po przebiegu. Dla E13 i E15: odmówiona edycja (`TagsNotLoaded`) unieważnia odczyt szczegółów zasobu będący w drodze (src-gpui/crates/media_tagger_client/src/library/library_model.rs:609), więc edytor nie wysyła edycji przed `Library::can_edit_tags`; `Display` błędów kolejki to angielskie komunikaty z Solid, widoki mapują warianty na klucze lokalizacji.
- Zostało: brak otwartych ustaleń P0–P3 i brak spornych; luka soczewek rundy 1 zamknięta. Niesprawdzone w działającej aplikacji (żaden widok nie używa modelu biblioteki). `test:all` uruchamia koordynator.
- Testy:
  - `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`: pass
  - `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`: pass
  - `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`: pass
  - `git diff --check`: pass
  - `job.py start E08 checks`: pass
