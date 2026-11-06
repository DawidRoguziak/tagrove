# Etap 01: kolejka zapisów per asset (bug „like i tagi nie działają”)

Fala A. Warstwa: TypeScript. Rust w tym etapie się nie zmienia.

Dokumentacja do przeczytania:
- `docs/subsystems/search-tags-and-media-groups.md`, zwłaszcza część o wspólnej blokadzie zapisu na assecie;
- `docs/subsystems/lightbox.md` (edycja);
- `docs/frontend/architecture-and-ui-conventions.md` (stan publikowany dopiero po sukcesie backendu).

## Objaw

Kliknięcie serca albo dodanie tagu czasem nic nie zmienia, pokazuje „failed” albo działa dopiero po kilku próbach. Zdarza się częściej podczas skanu i przy wolnym backendzie.

## Przyczyny (potwierdzone w kodzie)

1. **Drugi zapis na tym samym assecie jest odrzucany zamiast kolejkowany.**
   - `src/library/metadata/createMutations.ts:46-56`: `acquire()` rzuca wyjątek `"Metadata is busy. Retry after the current operation."`, gdy `tags.beginMutation(id)` zwraca `null`.
   - `src/library/metadata/createAssetTagState.ts:169-172` zwraca `null`, gdy na assecie trwa jakikolwiek zapis. Tagi, favorite i grupa mają wspólny token.
   - Lightbox kolejkuje tylko zapisy tagów między sobą (`src/features/lightbox/createLightbox.ts:239-262`). Favorite ma osobny guard (`createLightbox.ts:441-466`). Bulk ma osobne `tagLock`, `favoriteLock` i `groupLock` (`src/features/bulk/createBulk.ts:44-46, 141-192`).
   - Sekwencja: tag w locie → klik w serce → odrzucenie bez wywołania backendu → „favorite save failed”. UI publikuje stan dopiero po sukcesie, więc wizualnie nic się nie dzieje.
2. **Bulk nie ponawia odczytu detali po zmianie epoki zapytania** (`createBulk.ts:106-113`). Refresh w trakcie odczytu ustawia `tagDetailsFailed`, a kontrolki tagów zostają wyłączone. Lightbox ma pętlę ponowień (`createLightbox.ts:120-128`).
3. **Ciche no-op.**
   - `src/features/lightbox/LightboxModal.tsx:194-196`: tag ze spacją, przecinkiem lub średnikiem jest odrzucany bez komunikatu.
   - `addTag` wraca bez komunikatu, gdy `tagEditingDisabled`.
   - Bulk `saveTag` zwraca `false` po cichu przy `selection.busy` albo przy braku bazy tagów.

Etap 02 usuwa przyczyny po stronie backendu (pula, skan). Ten etap sprawia, że UI nie odrzuca poprawnych kliknięć.

## Kroki

1. **Kolejka per asset w `createMutations.write`.**
   - Zamiast `acquire()`, który rzuca wyjątek: mapa `id → Promise` (ogon łańcucha). `write(ids, …)` czeka na ogony wszystkich `ids`, potem bierze tokeny i wykonuje zapis. Wynik zapisu (sukces lub błąd) zwalnia ogon. Błąd jednego zapisu nie blokuje kolejnych.
   - Przy wielu ID zawsze bierz ogony w kolejności rosnących ID, aby uniknąć zakleszczeń między dwoma bulkami.
   - Wyjątek zostaje tylko wtedy, gdy aktywna jest bariera maintenance (restore, clear). Nie zmieniaj tej semantyki.
   - Kontrola `details.generation` (commit b7c2ef4) zostaje. Jeśli generacja zmieniła się podczas czekania w kolejce, odczytaj świeży stan bazowy przed zapisem zamiast zgłaszać „superseded”.
   - Opcjonalnie: limit długości kolejki per asset (np. 8), powyżej którego zwracany jest błąd z komunikatem.
2. **Uprość guardy w lightboxie i bulk.**
   - `createLightbox.ts`: favorite i tagi idą przez tę samą kolejkę z `createMutations`. Usuń osobny guard favorite. Zostaw tylko stan „pending” dla UI.
   - `createBulk.ts`: `tagLock`, `favoriteLock` i `groupLock` przestają odrzucać. Zostają jako flagi pending lub disabled tylko tam, gdzie UI tego potrzebuje (np. dwuklik tego samego przycisku).
3. **Ponowienie odczytu detali w bulk:** pętla wzorowana na `createLightbox.ts:120-128`, ponawiana przy zmianie epoki, z limitem prób.
4. **Komunikaty zamiast ciszy.** Niepoprawny tag dostaje komunikat walidacji (nowy klucz i18n we wszystkich 11 locale; `bun run locale:check`). Zablokowana edycja ma widoczny stan ładowania.
5. **Dokumentacja:** zaktualizuj `docs/subsystems/search-tags-and-media-groups.md`, część o blokadzie zapisu. Zamiast „odrzucany, gdy zajęty” ma być „kolejkowany per asset; odrzucany tylko przy barierze maintenance”.

## Pliki

- `src/library/metadata/createMutations.ts`, `createAssetTagState.ts`
- `src/features/lightbox/createLightbox.ts`, `LightboxModal.tsx`
- `src/features/bulk/createBulk.ts`
- `src/i18n/locales/*.json` (jeśli pojawią się nowe komunikaty)
- testy w `__tests__` obok powyższych plików

## Testy (Vitest)

- `src/library/metadata/__tests__/mutations.test.ts`:
  - zamień asercje „rejects when busy” (okolice linii 97, 133, 183) na „queues and runs in order”;
  - dodaj: błąd pierwszego zapisu nie blokuje drugiego;
  - dodaj: bariera maintenance nadal odrzuca.
- `src/features/lightbox/__tests__/createLightbox.test.ts`: `setAssetTags` jako deferred, `saveTags`, potem `toggleFavorite`. Oczekiwane: oba wywołania backendu w kolejności i oba stany opublikowane.
- `src/features/bulk/__tests__/createBulk.test.ts`:
  - `getAssetDetails` jako deferred, w trakcie `library.query.refresh()`, potem resolve. Oczekiwane: ponowny odczyt, brak `tagDetailsFailed`;
  - favorite i tag równolegle na tej samej selekcji.
- Komendy:
  `bun run test -- src/library/metadata src/features/lightbox/__tests__/createLightbox.test.ts src/features/bulk`

## Kryteria akceptacji

- Szybkie klikanie serca i dodawanie tagów na tym samym assecie zawsze kończy się zapisem wszystkich operacji w kolejności kliknięć.
- Brak komunikatu „Metadata is busy” poza barierą maintenance.
- Niepoprawny tag pokazuje komunikat.
- Zielone testy z sekcji Testy, `bun run typecheck`, `bun run lint`.

## Ryzyka

- Kolejka może ukryć wolny backend: użytkownik klika, a zapis wykonuje się po kilku sekundach. Rozwiązanie: wskaźnik pending na przycisku i etap 02.
- Zakleszczenie przy bulk z nakładającymi się zestawami ID. Rozwiązanie: stała kolejność ID.

## Status

todo — 2026-11-04

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->

### 2026-11-06 — orkiestrator (etap 07, krok 0)
- Z etapu 07: błąd odtworzony w prawdziwej aplikacji (`e2e/specs/grid-baseline.e2e.js`, build E2E debug). Scenariusz: podgląd, wpisanie nowego tagu, Enter i od razu klik w serce, 20 razy, pod obciążeniem skanem.
  - Like: 9/20, 12/20 i 15/20 nieudanych w trzech biegach. Za każdym razem bez wywołania `set_asset_favorite`, a UI pokazuje „The favorite change was not saved”. W biegu, w którym skan skończył się przed operacjami: 1/20.
  - Tag: 0/20 nieudanych we wszystkich biegach; zapis p50 35–129 ms, max 230–1348 ms (debug).
  - Backend w tym czasie nie zwrócił żadnego błędu zapisu (także 0 z 224 w teście Rust).
- Wniosek: objaw z sekcji „Objaw” to przyczyna 1 (odrzucenie zapisu favorite, gdy na assecie trwa zapis tagu). Skan tylko wydłuża okno wyścigu. Ten sam scenariusz po etapie 01 powinien dać 0/20 dla like i tagu (wiersz „Nieudane operacje” w tabeli etapu 07).
