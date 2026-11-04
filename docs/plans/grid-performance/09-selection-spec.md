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

todo — 2026-11-04

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->
