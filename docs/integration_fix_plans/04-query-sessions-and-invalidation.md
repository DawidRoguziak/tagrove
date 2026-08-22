# Etap 04 - Sesje zapytań i invalidation

## Metryka

- Priorytet: wysoki
- Status: Ukończony
- Zależności: Etap 03
- Następny etap: 05

## Cel

Zapewnić prawidłową semantykę latest-request-wins, spójny snapshot pierwszej strony oraz centralne odświeżanie membership i ordering po każdej mutacji widocznej dla galerii.

## Potwierdzone ryzyka

- Frontend przekazuje `generation`, ale backend ją ignoruje.
- Backend przydziela request ID dopiero wewnątrz pracy przekazanej do `spawn_blocking`.
- Scheduler może odwrócić kolejność rozpoczęcia dwóch żądań.
- Revision, lista ID i pierwsza strona nie są odczytywane w jednym snapshotcie.
- Zmiana tagów lub grupy patchuje lokalny obiekt bez ponownej oceny aktywnego filtra i kolejności.
- Część mutacji zapisuje dane i bumpuje revision w osobnych autocommitach.
- Błędy `refresh` i `loadPage` nie mają kontrolowanego stanu błędu i Retry.

## Zadania

- [x] Ujednolicić request ID/generation w kontrakcie TypeScript-Rust.
- [x] Zarejestrować kolejność żądania przed `spawn_blocking` albo oprzeć supersession na generation klienta.
- [x] Dodać kooperacyjne sprawdzanie anulowania podczas kosztownego budowania listy ID.
- [x] Odczytać revision, ordered IDs i pierwszą stronę z jednego snapshotu SQLite.
- [x] Zdefiniować jednoznaczne wyniki `ready`, `superseded`, `stale` i błąd.
- [x] Zbudować centralną ścieżkę invalidation dla scan, tag, favorite, group, rename, delete, CSV i restore.
- [x] Po mutacji wpływającej na filtr lub ordering uruchomić nową sesję zamiast wyłącznie patchować cache.
- [x] Połączyć każdą mutację widoczną w query z revision w tej samej transakcji.
- [x] Dodać stan błędu, Retry i kontrolowane ponawianie stron.
- [x] Nie oznaczać zakresu wirtualnego jako trwale obsłużony przed sukcesem strony.
- [x] Ograniczyć czas oczekiwania na connection pool i raportować stan busy.

## Kryteria odbioru

- Starsze żądanie nie może unieważnić nowszego przez kolejność schedulera.
- Pierwsza strona i sesja odpowiadają tej samej revision i temu samemu snapshotowi.
- Każda udana mutacja powoduje prawidłowy membership i ordering aktywnego widoku.
- Błąd strony nie tworzy trwałej dziury i jest możliwy do ponowienia.
- Bump revision nie może zostać utracony po zatwierdzonej mutacji.

## Planowana weryfikacja

- Kontrolowane odwrócenie kolejności dwóch requestów.
- Mutacje tag/favorite/group przy aktywnych filtrach i ordering grupowym.
- Błąd strony, stale session, eviction i Retry.
- Test realnego IPC dla generation po uzyskaniu osobnej zgody.

## Dziennik

### 2026-08-22 - sesja implementacyjna

- Status po sesji: Ukończony
- Zmienione pliki: `src-tauri/src/db.rs`, `src-tauri/src/models.rs`, `src-tauri/src/commands/assets.rs`, `src-tauri/src/commands/scan.rs`, `src-tauri/src/commands/import_export.rs`, `src-tauri/src/services/asset_query_service.rs`, `src-tauri/src/services/db_pool.rs`, `src-tauri/src/services/scan_service.rs`, `src/hooks/useLibraryAssets.ts`, `src/hooks/useLibraryBrowser.ts`, `src/hooks/useSelectionState.ts`, `src/components/app/hooks/useAppShellController.ts`, `src/components/app/hooks/useBulkSelectionController.ts`, `src/components/app/types.ts`, `src/components/app/AppGalleryView.tsx`, `src/components/app/services/libraryInvalidationService.ts`, `src/components/gallery/GalleryGrid.tsx`, `src/components/gallery/hooks/useGalleryVirtualGrid.ts`, `src/components/lightbox/services/toggleLightboxFavoriteAction.ts`, `src/i18n/locales/*.json` (klucze `gallery.loadError`/`gallery.retry`), testy: `src/hooks/__tests__/useLibraryAssets.test.tsx` (nowy), `src/components/app/services/__tests__/libraryInvalidationService.test.ts` (nowy), moduł testowy `asset_query_service.rs` (nowy) i nowe testy w `db.rs`; dokumentacja: `docs/architecture/ipc-contract.md`, `docs/subsystems/library-query-and-gallery.md`, `docs/subsystems/database.md`, `docs/subsystems/data-safety-and-portability.md`, `docs/integration_fix_plans/README.md`.
- Wdrożone zachowanie: komenda `start_asset_query` rejestruje żądanie (monotoniczny token + generation klienta) przed `spawn_blocking`, a supersession sprawdza oba znaczniki również na ścieżce cache-hit. Revision, ordered IDs i pierwsza strona powstają w jednym deferred odczycie SQLite; budowa listy ID współdzielone sprawdza anulowanie co 256 wierszy. Mutacje favorite/group/bulk-group/CSV/remove-root bumpują revision w tej samej transakcji co zapis, a skan bumpuje w każdej batchowej transakcji indeksowania. Frontend ma stan `loadError` z banerem i przyciskiem Retry (`retryLoad`), licznik `pageFailureEpoch` zwalniający marker zakresu wirtualnego po błędzie strony oraz centralną regułę invalidation (`libraryInvalidationService`): tag edit restartuje sesję przy zmianie tagów objętych filtrami, media-group edit zawsze restartuje sesję, favorite zachowuje dotychczasowy warunek favorites-only. Pool połączeń ogranicza oczekiwanie do 5 s i raportuje błąd `database pool is busy`.
- Decyzje i odchylenia: supersession oparto na kombinacji token rejestracji + generation klienta (zamiast wyłącznie generation), aby zachować poprawność między oknami; cache-hit również zwraca `superseded`, dzięki czemu przestarzające żądanie nigdy nie otrzymuje `ready`. Skan pozostaje partiami transakcji zgodnie z architekturą, ale każda partia niesie własny bump, więc przerwany skan nie zostawia niewidocznych zmian bez invalidacji. Bulk tag refresh używa warunku „updated>0 i aktywne filtry tagowe”, żeby nie wymuszać pełnego odświeżenia przy braku filtrów.
- Migracje i kompatybilność: brak migracji schematu. Kontrakt IPC nie zmienił kształtu odpowiedzi; jedyna zmiana semantyczna to wykorzystanie pola `generation`, które było wcześniej ignorowane. Modele `StartAssetQueryResult`/`AssetQueryPageResult`/`AssetSummary` zyskały `PartialEq` wyłącznie na potrzeby testów.
- Utworzone lub zmienione testy: nowe moduły `asset_query_service::tests` (9 testów), nowe testy `db.rs` (anulowanie i atomowe bumpi), nowy plik `useLibraryAssets.test.tsx` (6 testów), nowy `libraryInvalidationService.test.ts` (4 testy); zaktualizowano wywołania `set_assets_media_group_bulk(&mut conn, …)` w istniejących testach.
- Uruchomione sprawdzenia: pełny `bun run test:all` zakończony powodzeniem w kontenerze Docker (`media-tagger-stage03-e2e:local`, bo host nie ma Bun/Cargo): frontend 63 pliki / 336 testów zielonych, `cargo test` 106+1+5 testów zielonych, desktop E2E 2 specyfikacje (25 scenariuszy) zielone; dodatkowo `tsc --noEmit` i `git diff --check` czyste. W trakcie E2E wykryto i naprawiono zagnieżdżenie transakcji w CSV import (przejście na `set_asset_tags_in_tx` + pojedyncze sprzątanie osieroconych tagów przed commit).
- Niewykonane sprawdzenia i ryzyka: pulowy timeout busy nie ma dedykowanego testu jednostkowego; planowany "test realnego IPC dla generation po uzyskaniu osobnej zgody" jest pokryty pośrednio przez istniejące scenariusze E2E, ale brak dedykowanego testu odwrócenia kolejności dwóch requestów na poziomie WebView.
- Następny dokładny krok: w nowej sesji rozpocząć wyłącznie Etap 05 od ponownego odczytu jego planu i aktualnego kodu selekcji/cache stron.
