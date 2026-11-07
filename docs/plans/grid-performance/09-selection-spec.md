# Etap 09: zaznaczenie jako specyfikacja rozwiązywana w Rust

Fala E (po etapach 01 i 04). Warstwa: Rust (bulk) i TypeScript (`features/gallery/createSelection.ts`, `features/bulk`). Zaakceptowany przez użytkownika.

Dokumentacja do przeczytania:
- `docs/subsystems/search-tags-and-media-groups.md` (bulk selection, mutacje);
- `docs/subsystems/library-query-and-gallery.md`;
- `docs/architecture/ipc-contract.md`;
- `docs/subsystems/data-safety-and-portability.md`, jeśli bulk obejmuje usuwanie.

## Problem

Zaznaczenie to `Set<number>` (`src/features/gallery/createSelection.ts:12`). Każda zmiana kopiuje cały zbiór (`:69, 106, 143`). Select-all lub Shift-click przez 1M oznacza:
- 1M liczb w `Set` (dziesiątki MB);
- kopie przy każdym publish;
- JSON z 1M ID w każdej komendzie bulk.

Bulk liczy też `missing` przez kolejny `Set` (`src/features/bulk/createBulk.ts:184-185`).

## Projekt

- **Typ `SelectionSpec`:**
  ```
  { sessionId, revision, ranges: [start, endInclusive][], excluded: number[] /* ID */, included: number[] /* ID spoza zakresów */ }
  ```
  - zakresy są w przestrzeni indeksów sesji, posortowane i scalone;
  - `excluded` i `included` przechowują pojedyncze kliknięcia z Ctrl.
- **Frontend.**
  - `createSelection` trzyma spec.
  - `isSelected(index, id)` to wyszukiwanie binarne po zakresach plus sprawdzenie w małych zbiorach: O(log r).
  - Licznik zaznaczonych liczony bez materializacji: suma długości zakresów − |excluded ∩ zakresy| + |included|.
  - Shift-click i prostokąt dodają zakres bez pobierania ID. Etap 04 (komenda zakresu ID) zostaje tylko tam, gdzie ID są naprawdę potrzebne.
- **Backend.**
  - Bulk-komendy (tagi, favorite, grupa i inne, które dziś przyjmują `ids`) dostają wariant ze `spec`.
  - Rust rozwiązuje spec na wektorze sesji. Sprawdza rewizję; przy niezgodności odpowiada `stale` i nie wykonuje częściowej operacji. To ta sama semantyka co dzisiejsze „odrzuć cały zakres” w `getIdsRangeAsync`.
  - Rozwiązanie i zapis odbywają się w jednej transakcji z bumpem rewizji. Duże zestawy idą partiami w obrębie jednej operacji z postępem, jeśli istniejący mechanizm postępu na to pozwala.
- **Zmiana zapytania przy aktywnym zaznaczeniu.** Spec jest związany z sesją. Nowa sesja (zmiana filtra) wymaga albo zmaterializowania małego zaznaczenia (≤ N ID, np. 10k) do `included`, albo jego wyczyszczenia z komunikatem. Wybierz jedno, opisz w dokumentacji i przetestuj.
- **Kolejka per asset z etapu 01:** bulk ze specem bierze ogon kolejki dla „całej biblioteki” albo dla rozwiązanych ID. Wybór i uzasadnienie zapisz w notatkach. Ważne: nie wolno doprowadzić do zakleszczenia z pojedynczymi zapisami.

## Pliki

- `src/features/gallery/createSelection.ts`, `selection.ts`, `GalleryGrid.tsx` (odczyt `isSelected`), `createGridHandlers.ts`
- `src/features/bulk/createBulk.ts`, `BulkActionsSidebar.tsx`
- `src/library/metadata/createMutations.ts` (wariant ze spec)
- `src/platform/tauri/*`, `contracts/*`
- `src-tauri/src/commands/assets.rs`, `src-tauri/src/services/asset_query_service.rs`, `src-tauri/src/db/mutations.rs`, `src-tauri/src/lib.rs`
- `docs/subsystems/search-tags-and-media-groups.md`, `docs/architecture/ipc-contract.md`

## Testy

- Vitest `createSelection.test.ts`:
  - scalanie zakresów;
  - Ctrl-klik wewnątrz zakresu (do `excluded`) i poza nim (do `included`);
  - licznik;
  - `isSelected` dla 1M bez materializacji;
  - zmiana zapytania.
- Vitest `createBulk.test.ts`: favorite i tag na specu; stale odrzuca w całości.
- Rust `backend_integration`:
  - rozwiązanie specu (zakresy, wyłączenia);
  - rewizja niezgodna oznacza brak zapisu;
  - 1M w teście `#[ignore]` z czasem.
- E2E (orkiestrator): zaznacz wszystko, dodaj tag, a licznik i wynik filtra po tagu się zgadzają.

## Kryteria akceptacji

- Select-all i Shift-click przez 1M: brak zauważalnego opóźnienia UI (< 50 ms). Pamięć JS nie rośnie proporcjonalnie do liczby zaznaczonych.
- Bulk tag na 1M kończy się poprawnie albo odrzuca całość przy zmianie rewizji.

## Ryzyka

- Zakres semantyczny: zaznaczenie musi znaczyć dokładnie to, co widział użytkownik. Rewizja w specu jest obowiązkowa.
- Duża operacja bulk trzyma write lock długo i może blokować pojedyncze zapisy (etap 01/02). Rozważ partie z commitem pośrednim tylko wtedy, gdy dokumentacja data-safety na to pozwala.

## Status

done — 2026-11-07 (czeka na akceptację i commit)

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->

### 2026-11-07 — agent etapu 09
- Zrobione:
  - `createSelection` trzyma spec: posortowane i scalone zakresy sesji plus małe mapy `excluded` (ID w zakresach, niezaznaczone) i `included` (ID poza zakresami). `isSelected(index, id)` to wyszukiwanie binarne po zakresach i lookup w mapie, licznik = pokrycie − excluded + included. Ctrl+A w trybie bulk zaznacza cały wynik jednym zakresem (`select-all` w `createGridHandlers.ts`, poza polami tekstowymi i nad modalami).
  - Backend: `SelectionSpec` i `resolve_selection` w `asset_query_service.rs` (bez SQL, walidacja zakresów), `selection_service.rs` z komendą `apply_selection_mutation(selection, mutation)` dla `mergeTags`, `toggleFavorite`, `setMediaGroup`. Zapis w jednej transakcji IMMEDIATE (`db::guarded_user_write`): najpierw sprawdzenie rewizji zależności sesji, potem zapis i bump. Niezgodność albo brak sesji → `{ status: "stale" }`, nic nie jest zapisane. Pełna zmiana IPC, [ADR 0008](../../adr/0008-selection-spec-resolved-in-rust.md).
  - Funkcje bulk w `db/mutations.rs` rozdzielone na wersje `_in_tx` (favorite, grupa, merge tagów) używane przez komendy ID i przez spec.
  - Frontend: `createMutations.mergeTagsInSelection/toggleFavoritesInSelection/setMediaGroupInSelection`, `createBulk` wybiera spec, gdy zaznaczenie ma zakresy; sidebar pokazuje licznik bez materializacji i alert „Selection cleared” (`bulk.panel.selectionCleared*` w 11 językach); `GalleryGrid` czyta `isSelected(index, id)`.
- Decyzje:
  - **Zmiana zapytania:** zakresy ≤ 10 000 pozycji (`MATERIALIZE_LIMIT`) są od razu rozwiązywane jednym binarnym odczytem do `included`, więc małe zaznaczenia zachowują się jak dotąd i przeżywają zmianę filtra. Większe zostają zakresami; gdy sesja się zmieni (filtr, skan, zmiana kolejności, każdy nowy `session_id`), całe zaznaczenie jest czyszczone z komunikatem. Ta sama sesja z cache (to samo ID) zachowuje zakresy. Opisane w `search-tags-and-media-groups.md` i przetestowane (`createSelection.test.ts`).
  - **Rewizja w specu:** spec niesie `sessionId`, nie publiczną rewizję. Sesja jest ważna, dopóki jej rewizja zależności się nie zmieni (ta sama reguła co dla stron). Publiczna rewizja unieważniałaby spec po każdym niezwiązanym zapisie, także po własnym poprzednim zapisie tagu.
  - **Kolejka per asset (etap 01):** zapis ze specem bierze ogon „całej biblioteki” (`enqueueLibraryMutation` w `createAssetTagState.ts`): czeka na wszystkie wcześniejsze zapisy, a każdy późniejszy czeka na niego. Graf zależności dalej idzie w kolejności admisji, więc zakleszczenie z pojedynczymi zapisami jest niemożliwe. Rozwiązanych ID frontend nie zna, dlatego tura unieważnia wszystkie pełne listy tagów i szczegóły (na początku i na końcu, by odrzucić odczyty z czasu zapisu).
  - **Partie:** bez commitów pośrednich (naruszyłyby „wszystko albo nic”). Zamiast tego merge tagów dla specu jest zbiorowy (tabela tymczasowa ID, jeden `INSERT … SELECT` na tag, dokładne przeliczenie `tag_count` dla assetów, które dostały tag).
  - Grupa na specu nadaje kolejność `1..N` w kolejności galerii; edytor kolejności wymaga ID i nie jest oferowany dla zakresów. Grupowanie zmienia kolejność, więc następny zapis na tym samym specu jest `stale`, a zaznaczenie znika z komunikatem.
- Odkryte:
  - Merge tagów per asset na 1M w jednej transakcji: 35 s (≈10 poleceń na asset). Blokada zapisu na tyle przekracza 5 s busy timeout zapisów w tle. Zbiorowy merge: 2,8 s; toggle favorite 1,9 s; rozwiązanie specu 15 ms (release, `one_million_select_all_tag_and_favorite`). Zbiorowy merge nie kanonizuje starej pisowni tagów każdego assetu (naprawa zostaje na ścieżkach per asset).
- Zostało: przy 1M zapis trzyma blokadę zapisu ~2–3 s; zapisy w tle czekają w swoim busy timeout. Desktopowy pomiar select-all przy 1M nie istnieje (fixture E2E ma 12 005 plików).
- Testy:
  - `cargo test --locked --test backend_integration selection_spec` → 2 pass (+1 ignored); `--release … one_million_select_all -- --ignored` → resolve 15 ms, merge 2769 ms, favorite 1945 ms; `--lib selection_service` (kształt payloadu) → pass.
  - Vitest: `createSelection.test.ts` (scalanie, Ctrl-klik w/poza zakresem, licznik, 1M bez materializacji < 50 ms, zmiana zapytania, nieznane pozycje ID z poprzedniego zapytania), `createBulk.test.ts` (tag/favorite/grupa na specu, stale bez zapisu), `createAssetTagState.test.ts` (tura biblioteki w obie strony, utrzymanie, reset), `createGridHandlers.test.tsx` (Ctrl+A), `BulkActionsSidebar.test.tsx`, `api.test.ts` → pass.
  - E2E `gallery-timeline.e2e.js`, przypadek select-all: Ctrl+A → „Selected: 12005”, Ctrl-klik → 12004, tag jednym zapisem 388–550 ms, filtr po tagu zwraca 12004, wykluczony asset bez tagu, alert po zmianie sesji → pass.
