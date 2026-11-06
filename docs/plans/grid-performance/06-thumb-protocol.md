# Etap 06: własny protokół `thumb://` zamiast galeryjnej kolejki miniatur

Fala C, po etapie 05. Warstwa: Rust i TypeScript. Decyzja użytkownika: tak, ale zaczynamy od spike'u. Jeśli spike się nie powiedzie, realizujemy plan B z tego pliku.

Dokumentacja do przeczytania:
- `docs/subsystems/thumbnails.md`;
- `docs/architecture/ipc-contract.md` (miniatury i kanały);
- `docs/architecture/system-overview.md` (powierzchnia bezpieczeństwa, profile, blokady).

## Jak jest teraz

1. Strona galerii zwraca `AssetSummary` z `thumb_path` (ścieżka absolutna albo `null`). Kafel robi z niej URL przez `convertFileSrc`, a plik serwuje asset protocol.
2. Przy braku ścieżki kafel zgłasza ID do `src/library/thumbnails/createThumbnailQueue.ts`. Kolejka:
   - po 36 ms wysyła paczkę do 64 ID przez `ensure_thumbnails` (Channel);
   - **czeka na całą paczkę** (`createThumbnailQueue.ts:84-116`, `src-tauri/src/services/thumb_service.rs:837-882`);
   - po 24 ms aktualizuje `thumbnailStore`.
3. Problemy:
   - `request_id` jest ignorowany (`thumb_service.rs:645`);
   - kolejka high-priority to FIFO (`src-tauri/src/services/thumb_scheduler.rs:399-421`);
   - jedno wideo może zablokować nowy widok na ~45 + 45 s (`src-tauri/src/thumbs.rs:16`);
   - `refresh()` czyści store (do czasu etapu 04).

## Docelowo

```
<img src="thumb://localhost/<id>?v=<version>&s=<size>" crossorigin="anonymous" decoding="async">
```

Handler Rust (`register_asynchronous_uri_scheme_protocol`):
1. Parsuje **tylko** numeryczne `id`, `v` i `s` (rozmiar z białej listy). Nigdy nie przyjmuje ścieżki.
2. Czyta z SQLite źródło i `SourceVersion` assetu (krótki odczyt z puli).
3. Jeśli plik miniatury dla tej wersji istnieje, zwraca `200 image/jpeg` z `Cache-Control: public, max-age=31536000, immutable` (URL zmienia się wraz z `v`).
4. Jeśli pliku nie ma, zgłasza zadanie do `ThumbnailScheduler` z priorytetem **LIFO**: najnowsze żądanie pierwsze, z limitem równoległości dla ffmpeg. Gdy zadanie się skończy, zwraca plik. Pilnuje compare-and-set na `SourceVersion` i blokad z etapu 02.
5. Błąd lub nieobsługiwany typ daje 404, a frontend w `onerror` pokazuje placeholder lub ikonę. Przeciążenie daje 503 z `Retry-After`; frontend ponawia z backoffem.
6. Ustawia nagłówki CORS (`Access-Control-Allow-Origin` dla originu dev `http://localhost:1420` i originu produkcyjnego), bo `ThumbnailImage` używa `crossorigin="anonymous"` (commit 10e50f6).

`v` to krótki hash `SourceVersion` (ścieżka, rozmiar, `fingerprint_mtime_ns`) i wersji renderera. Przychodzi w `AssetSummary` jako nowe małe pole w miejsce `thumb_path`.

## Krok 0: spike (przed jakimkolwiek usuwaniem)

Minimalny handler pod flagą, sprawdzony na prawdziwym WebKitGTK (E2E przez orkiestratora). Pytania:
1. **Anulowanie.** Czy WebKitGTK anuluje żądanie do własnego schematu, gdy `<img>` zniknie z DOM? Czy Tauri lub wry daje na to sygnał (zamknięty responder, drop)? Jeśli nie, kafel przy odmontowaniu wysyła lekkie `thumb_demote(ids)` (batch co rAF), a scheduler obniża priorytet tych zadań.
2. **CORS i originy:** dev (`http://localhost:1420`), build (`tauri://localhost` lub `http://tauri.localhost`), E2E.
3. **Równoległość:** ile żądań naraz WebKit wysyła do własnego schematu. Czy handler async nie blokuje głównego wątku?
4. **Cache:** czy WebKit respektuje `immutable` dla własnego schematu (brak ponownych żądań po remount).
5. **Przepustowość:** czas obsługi istniejącej miniatury (cel < 2 ms) w porównaniu z asset protocol.

Wynik spike'u wpisz do notatek z decyzją **GO** albo **plan B**.

## Kroki (GO)

1. Handler `thumb://`: moduł w `src-tauri/src/commands/` lub `services/`, rejestracja w builderze w `src-tauri/src/lib.rs`. Kolejność startu według `system-overview.md`. Uwzględnij izolację profili (katalog `thumbs/` aktywnego profilu).
2. Scheduler:
   - priorytet LIFO dla żądań protokołu;
   - limit równoległości osobno dla obrazów (CPU) i wideo (ffmpeg);
   - demote i cancel według wyniku spike'u;
   - bulk renderer z ustawień działa dalej z niższym priorytetem.
3. `AssetSummary`: zamiast `thumb_path` pole `thumb_version` (lub podobne). Zmiana IPC: Rust model, `contracts/`, `ipc-contract.md`.
4. Frontend:
   - `ThumbnailImage` i `GalleryTile` budują URL z `id` i `thumb_version`;
   - stany ładowania, błędu i retry w komponencie;
   - usunięcie `createThumbnailQueue.ts`, `thumbnailStore.ts`, `createAssetThumbnail.ts` oraz ich użyć (`createLibrary`, `createAssetQuery`, `AppGalleryView`, stopka „generating thumbnails”: nowe źródło liczby pending albo usunięcie stopki, w uzgodnieniu z i18n).
   - Uwaga: sprawdź przez `rg ensureThumbnailsStream|thumbnailStore|thumb_path src`, czy duplikaty, kolejność grup i lightbox nie zależą od tych modułów. Jeśli tak, przenieś je na protokół w tym samym etapie.
5. Asset protocol: usunąć `$APPDATA/thumbs/**` ze scope, jeśli nic go już nie używa. Szerokie `"**"` zostaje dla lightboxa; jego zawężenie to osobny temat bezpieczeństwa, wpisz go do notatek.
6. `ensure_thumbnails` zostaje tylko dla operacji z ustawień (bulk). Usuń galeryjną ścieżkę page i `_request_id`.
7. **Dekodowanie** (`src-tauri/src/thumbs.rs:79-97`):
   - jedno otwarcie pliku (bez osobnego probe);
   - dla JPEG skalowanie przy IDCT (1/2–1/8) do najmniejszej skali ≥ rozmiaru docelowego;
   - zamiast Lanczos3 tańszy filtr (np. Triangle lub CatmullRom) na już zmniejszonym obrazie.

   Jeśli wymaga to nowego crate'a (np. dekoder z obsługą skalowania), **zapytaj użytkownika** przed zmianą `Cargo.lock`. Zmierz zysk na syntetycznych JPEG 24 MP.
8. **Wideo:** przejrzyj argumenty ffmpeg (`thumbs.rs` około 159): `-ss` przed `-i`, `-skip_frame nokey`, `-frames:v 1`, `-an -sn`, limit wątków. Zmierz.
9. **HiDPI:**
   - sprawdź maksymalny rozmiar kafla (`TileSizeSlider.tsx`, `tileSizeService.ts`) × `devicePixelRatio` wobec 390 px (`thumbs.rs:12`);
   - jeśli to przekracza 390 px, dodaj drugi rozmiar (np. 780) wybierany parametrem `s`, osobny plik w `thumbs/`, zgodny z cleanupem i backupem;
   - zaktualizuj `thumbnails.md` i `data-safety-and-portability.md`.

## Plan B (spike negatywny): naprawa obecnej kolejki

- Backend: wyniki per element (już przez Channel) bez czekania na całą paczkę; prawdziwe `request_id` z `thumb_cancel(requestId)` dla ID, które opuściły viewport; LIFO dla high-priority.
- Frontend: osobne wywołanie dla prefetchu i dla widocznych; do 2 wywołań w locie; przegląd timerów 36/24 ms; zachowanie ścieżek per asset ID przy `refresh`.
- Kroki 7–9 (dekodowanie, wideo, HiDPI) wykonaj tak samo.

## Pliki

- `src-tauri/src/lib.rs`, `src-tauri/src/services/thumb_service.rs`, `thumb_scheduler.rs`, `src-tauri/src/thumbs.rs`, `src-tauri/src/commands/thumbs.rs`, model `AssetSummary` w `src-tauri/src/db/`
- `src-tauri/tauri.conf.json` (scope asset protocol), ewentualnie `tauri.conf.dev.json` i `tauri.conf.e2e.json`
- `src/ui/controls/ThumbnailImage.tsx`, `src/features/gallery/GalleryTile.tsx`, `src/library/thumbnails/*`, `src/library/createLibrary.ts`, `src/library/query/createAssetQuery.ts`, `src/app/AppGalleryView.tsx`, `src/platform/tauri/*`
- `docs/subsystems/thumbnails.md`, `docs/architecture/ipc-contract.md`, `docs/architecture/system-overview.md`, `docs/subsystems/data-safety-and-portability.md` (jeśli pojawi się drugi rozmiar)

## Testy

- Rust unit handlera:
  - parsowanie (odrzuca ścieżki, nienumeryczne `id`, nieznane rozmiary);
  - statusy 200, 404 i 503;
  - niezgodna wersja;
  - nagłówki cache i CORS;
  - limit równoległości;
  - LIFO.
- `cargo test --locked --test thumbnail_bulk`, `--test thumbnail_clear` (cleanup nie usuwa pliku w trakcie serwowania).
- Vitest: `ThumbnailImage` (URL z wersji, `onerror`, retry po 503, brak nowego `<img>` przy zmianie `src`).
- Desktop E2E (orkiestrator):
  - miniatury pojawiają się dla nowej biblioteki;
  - po przewinięciu i powrocie nie ma ponownych żądań;
  - delete i rename nie psują widoku.
- Pomiar dekodowania przed i po (test diagnostyczny `#[ignore]`).

## Kryteria akceptacji

- Nowe kafle na ekranie dostają miniatury przed przewiniętymi (LIFO). Żadne wideo nie blokuje innych miniatur.
- Po `refresh` i po remount kafla brak pustych miniatur.
- Usunięte moduły kolejki i `thumb_path` (GO) albo zrealizowany plan B.
- Generowanie miniatury JPEG 24 MP przynajmniej ~2× szybsze.

## Ryzyka

- Własny schemat to nowa powierzchnia ataku. Tylko numeryczne parametry, brak ścieżek, brak list katalogów. Opisz to w `system-overview.md`.
- Brak sygnału anulowania w WebKitGTK oznacza konieczność demote z frontendu (spike to rozstrzyga).
- Handler musi działać w async i nie trzymać blokad przez `.await` (zasada z `CLAUDE.md`).

## Status

todo — 2026-11-04

## Notatki agentów

<!-- dopisuj wpisy według protokołu z README -->

### 2026-11-06 — orkiestrator (etap 07, krok 0)
- Z etapu 07: wartości bazowe obecnej kolejki miniatur.
  - Miniatura JPEG 24 MP (6000×4000, release, `thumbs::create_image_thumb`): 99 ms. Kryterium „~2× szybciej” oznacza ≤ ~50 ms.
  - Pusty cache, build E2E debug, pliki 320×240: kafle z danymi 180 ms po skoku, miniatura per kafel p50 354 ms, p95 444 ms, max 485 ms, komplet widocznych p50 449 ms.
  - `render_all_thumbnails` dla 20 243 plików: 82 s w debug.
- Czasy w E2E obejmują niezoptymalizowany Rust (profil debug). Porównania po zmianie rób tym samym specem i profilem.
