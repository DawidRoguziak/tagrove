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

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E07

- Z epiku E07: model zapytań to `media_tagger_client::library::AssetQuery`, osadzony w `AppRuntime` (`RuntimeInput::Query`, `RuntimeEffect::Query`, runner przez `run_query_effect(backend, effect, dispatch, wrap)`; ADR 0040). Modele tego epiku osadzaj tak samo, obok zapytania. Nie ma callbacku `onQueryChanged`: cache szczegółów czyści się, gdy zmieni się `query.query_epoch()` (porównaj po każdym wejściu przekazanym do zapytania).
- Z epiku E07: `patchAssets` to `QueryInput::PatchAssets(AssetPatch)`: `AssetPatch::Update(Vec<(id, SummaryPatch)>)` z polami `is_favorite`, `media_group_key`, `media_group_order` (bez tagów, jak w Solid) i `AssetPatch::Remove(ids)`. Nowe pole podsumowania w łatce dopisz w `library/page_cache.rs` (`SummaryPatch::apply`). Odświeżenie po mutacji to `QueryInput::Refresh`; reguły są gotowe w `library::tag_mutation_requires_refresh` i `favorite_mutation_requires_refresh` (filtry z `query.filters()`).
- Z epiku E07: `createLibrary` jest przeniesione tylko w części zapytania (filtry, zakres, kotwica). `refreshLibrary` (zapytanie + słownik), `handleLibraryCleared` (`QueryInput::Reset`, potem `QueryInput::SetViewport(None)`, reset słownika i tożsamości), `handleImportDbRestored` (`QueryInput::Reset` bez zmiany zakresu) i `runMaintenance` (bariera mutacji) należą do tego epiku; przypadek „clears assets and tags on library reset” z `createLibrary.test.ts` da się domknąć dopiero ze słownikiem.
