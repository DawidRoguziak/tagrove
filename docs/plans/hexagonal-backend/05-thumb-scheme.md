# Etap 05: podział `thumb://`

Fala B. Warstwa: Rust (protokół `thumb://`, nowy plik adaptera, rejestracja schematu w `lib.rs`) i dokumentacja. Kontrakt wire bez zmian.

Dokumentacja do przeczytania:
- [README planu](README.md) (sekcja 05, „Fale i równoległość”, „Zasady dla agentów”, wiersz o `http` w tabeli Decyzje);
- [ADR 0034](../../adr/0034-tauri-independent-backend-core.md), [ADR 0006](../../adr/0006-thumb-uri-scheme.md), [ADR 0021](../../adr/0021-visible-tiles-drop-a-pending-prefetch.md);
- `docs/architecture/system-overview.md` (startup order, tabela modułów, „Managed state”);
- `docs/subsystems/thumbnails.md` („The `thumb://` protocol”, scheduler);
- `docs/architecture/ipc-contract.md` (sekcja o `thumb://`).

## Cel

Usunąć `services/thumb_protocol.rs` z `FRAMEWORK_ALLOWLIST` w `src-tauri/tests/architecture_boundaries.rs` (4 → 3 wpisy, zostają same wideo). Rdzeń trzyma parser, decyzje i Publishera na danych, a adapter trzyma tylko Tauri. Zachowanie (LIFO, anulowanie prefetchu, okno 503, nagłówki) się nie zmienia.

## Problemy (potwierdzone w kodzie, commit `7acddc9`)

- `services/thumb_protocol.rs:19-22`: `tauri::{http::{header, Request, Response, StatusCode}, AppHandle, Manager, Runtime, UriSchemeResponder}`.
- `:108-152`: `empty`, `image_response`, `not_found`, `unavailable`, `bad_request` budują `tauri::http::Response<Cow<'static, [u8]>>`.
- `:156-177`: `handle` (wątek GTK) czyta `Origin` i metodę, parsuje i woła `tauri::async_runtime::spawn_blocking`.
- `:179-240`: `serve` bierze `AppHandle<R>`, `try_state::<AppState>()` (okno 503) i `UriSchemeResponder` w slocie.
- `:245-337`: `Finished` trzyma `UriSchemeResponder`, `Publisher::push` woła `tauri::async_runtime::spawn_blocking`, a `publish_batch` woła `app.try_state::<AppState>()`.
- Użytkownicy:
  - `lib.rs:56-62`: rejestracja i komentarz;
  - komentarz `commands/thumbs.rs:16`;
  - komentarz `thumb_service.rs:3160` (zostaje, bo nadal wskazuje testy `thumb_protocol`).
- Cele `src-tauri/tests` (`thumbnail_bulk`, `thumbnail_clear`, `grid_baseline`, `backend_runtime`, `architecture_boundaries`/`dto_snapshot`) nie importują `thumb_protocol`. Jedyne trafienie to wpis allowlisty.
- `http` nie jest bezpośrednią zależnością w `Cargo.toml`, więc `use http::` w rdzeniu się nie skompiluje. `tauri::http` łapie reguła 1.
- Końce linii:
  - wszystkie edytowane pliki mają LF (0 linii CRLF);
  - `thumb_service.rs` (423 linie CRLF) i `thumb_scheduler.rs` (459) mają końce mieszane, a w tym etapie są dotykane tylko próbami mutacyjnymi.

## Rdzeń i adapter

Granica:

- **LIFO, priorytety i promocja prefetchu** (ADR 0021):
  - `thumb_service.rs:824` wybiera Interactive albo High;
  - `thumb_scheduler.rs` robi `push_front` dla Interactive i przesuwa job dołączony jako Interactive na przód kolejki.
  - Oba pliki są rdzeniem i się nie zmieniają.
- **Mapowanie flag `c`/`p` na `ThumbnailRequestMode`** jest w rdzeniu (`serve`).
- **Nagłówki i status** (CORS, `Cache-Control` immutable/no-store przez porównanie wersji, `Retry-After` przy 503, kolejność nagłówków) powstają w rdzeniu jako dane `ThumbReply`. Adapter przepisuje je 1:1 do `tauri::http::Response`.
- **Gwarancja odpowiedzi na każde żądanie** (slot z responderem i Publisher, który odpowiada każdemu z partii) jest w rdzeniu.
- **Okno 503:**
  - adapter podaje fakt: `TauriThumbHost::app_state` = `try_state::<AppState>()`, czyli `None`, dopóki `setup` nie zrobi `manage(AppState)`;
  - rdzeń zamienia `None` na `unavailable` w `serve` i w `publish_batch`; ten kod jest przeniesiony dosłownie;
  - to doprecyzowanie README („503 zanim istnieje `AppState`” w adapterze): źródło faktu jest w adapterze, a odpowiedź w rdzeniu, więc okno da się przetestować bez Tauri.

Rdzeń, `services/thumb_protocol.rs`:
- `SCHEME`, stałe nagłówków i statusów, `ThumbRequest`, `parse_request` (bez zmian), `allowed_origin` (teraz `pub`).
- `ThumbReply { status: u16, headers: Vec<(&'static str, &'static str)>, body: Vec<u8> }` oraz `header(name)`. Nazwy nagłówków pisane małymi literami.
- Porty:
  - `pub type ThumbResponder = Box<dyn FnOnce(ThumbReply) + Send>`;
  - `pub trait ThumbHost: Clone + Send + 'static { fn app_state(&self) -> Option<&AppState>; fn spawn_blocking<F: FnOnce() + Send + 'static>(task: F); }`.
- `serve<H: ThumbHost>`, `Finished`, `Publisher` (static `OnceLock`, niegeneryczny), `push<H>`, `drain<H>`, `publish_batch<H>`, `take`, `PUBLISH_BATCH = 64`. Zmienione tylko typy:
  - `responder.respond(x)` → `responder(x)`;
  - `app.try_state::<AppState>()` → `host.app_state()`;
  - `tauri::async_runtime::spawn_blocking` → `H::spawn_blocking`.

Adapter, `src/adapter/thumb_scheme.rs`:
- `handle<R: Runtime>` (wątek GTK):
  - czyta `Origin`;
  - metoda inna niż GET albo zły URL → `bad_request` od razu;
  - potem `tauri::async_runtime::spawn_blocking(serve(TauriThumbHost(app), …, Box::new(|reply| responder.respond(into_response(reply)))))`.
- `TauriThumbHost<R>(AppHandle<R>)`: newtype z ręcznym `Clone`, implementuje `ThumbHost`.
- `into_response`: `Response::builder().status(u16)`, nagłówki w kolejności, `Cow::Owned(body)`, `.expect("static response parts are valid")`. Pusta treść to teraz `Cow::Owned(Vec::new())` zamiast `Cow::Borrowed(&[])`: bez alokacji i te same bajty.

Port: closure dla respondera, trait dla hosta.
- **Responder jako closure.** Ma jedną operację (odpowiedz raz) i jedną implementację. Trait nic nie dodaje, a `impl` traitu rdzenia na `UriSchemeResponder` po wydzieleniu crate'a (etap 08) łamie regułę sierot (E0117).
- **Host jako trait.** Daje dwie powiązane rzeczy (stan i pulę wątków). `app_state` zwraca referencję o czasie życia hosta, czego closure nie wyrazi prosto. `spawn_blocking` jest generyczne, więc nie ma dodatkowego `Box` na zadanie. Implementacja jest na newtypie w adapterze, więc E0117 go nie dotyczy.

Odrzucone:
- `trait ThumbResponder { fn respond(self: Box<Self>, reply) }` z `impl` na `UriSchemeResponder`: więcej kodu dla jednej implementacji, a w 08 E0117 wymusiłby newtype.
- `impl ThumbHost for AppHandle<R>` bez newtypa: ten sam E0117 w 08.
- Publisher generyczny po typie respondera albo hosta: `static` nie może być generyczny, a osobny Publisher na typ zmieniłby grupowanie publikacji.
- `http` w rdzeniu: decyzja z README (nowa bezpośrednia zależność).
- `std::thread::spawn` w rdzeniu zamiast portu: zmiana puli wątków, czyli zmiana logiki.
- Sprawdzanie `AppState` tylko w adapterze, z `serve(&AppState, …)`: `publish_batch` i tak potrzebuje stanu na wątku Publishera, a okno 503 nie dałoby się przetestować bez Tauri.

## Kroki

1. Ten plik; status 05 w README → `in progress`.
2. Pomiar bazowy: `--lib` i `--test architecture_boundaries`.
3. `services/thumb_protocol.rs`: `ThumbReply`, porty, `serve` i Publisher na portach; testy przepisane na `ThumbReply` plus moduł `serving`.
4. `adapter/thumb_scheme.rs` (`handle`, `TauriThumbHost`, `into_response`, test) i `pub mod thumb_scheme;` w `adapter/mod.rs`.
5. `lib.rs`: rejestracja woła `adapter::thumb_scheme::handle`, `SCHEME` zostaje z `services::thumb_protocol`, komentarz wskazuje adapter.
6. Komentarz `commands/thumbs.rs:16`.
7. `FRAMEWORK_ALLOWLIST`: usunięty `services/thumb_protocol.rs` (zostają 3 wpisy).
8. Dokumentacja (lista w [Pliki](#pliki)).

## Pliki

- Nowe:
  - `src-tauri/src/adapter/thumb_scheme.rs`
  - ten plik
- Zmienione:
  - `src-tauri/src/services/thumb_protocol.rs`
  - `src-tauri/src/adapter/mod.rs`
  - `src-tauri/src/lib.rs` (rejestracja schematu i jej komentarz)
  - `src-tauri/src/commands/thumbs.rs` (komentarz)
  - `src-tauri/tests/architecture_boundaries.rs` (tylko `FRAMEWORK_ALLOWLIST`)
  - `docs/architecture/system-overview.md`: startup order punkt 1 (503), tabela modułów (`adapter/*`, `services/*`), bezpieczeństwo `thumb://`
  - `docs/subsystems/thumbnails.md`: entry points, akapit handlera, „Relevant tests”
  - `docs/architecture/ipc-contract.md`: punkt 5 checklisty testów `thumb://`
  - `.claude/rules/thumbnails.md`, `.claude/rules/backend-core.md`: `paths:` + `src-tauri/src/adapter/thumb_scheme.rs`, wskazanie pliku
  - `.claude/skills/audit/references/batches.json`: batch B2b + `adapter/thumb_scheme.rs`
  - `README.md` (status, sekcje 07 i 08: `Z etapu 05`) i `NOTES.md` planu
- Bez zmian:
  - `COMMAND_SIGNATURES`, `REGISTERED_COMMANDS`, `dto_snapshot`, nazwy eventów;
  - `Cargo.toml`, `Cargo.lock`, frontend;
  - ADR 0006 (Accepted; wskazuje plik historycznie) i ADR 0034 (Context opisuje stan na `397181f`, a Decision już mówi, że rdzeń zwraca dane, które adapter zamienia na `tauri::http::Response`);
  - `docs/development/testing.md` (nie wskazuje `thumb_protocol`).
  - Nowego ADR nie ma: kształt portu to szczegół w ramach ADR 0034, zapisany razem z alternatywą tutaj.

## Testy

- Stan wyjściowy (`7acddc9`): `--lib` 360 passed, 2 ignored; `architecture_boundaries` 23 passed, 1 ignored.
- Przepisane na `ThumbReply` (nazwy i asercje bez zmian):
  - `parses_numeric_id_and_hex_version`;
  - `rejects_paths_and_malformed_parameters`;
  - `origin_is_echoed_only_when_allowed`;
  - `matching_version_is_immutable_and_stale_url_is_not_cached`;
  - `legacy_file_with_empty_version_is_not_cached`;
  - `error_responses_are_uncached_and_503_asks_for_retry`.
- Nowe w rdzeniu (`thumb_protocol::tests::serving`, bez Tauri). Fałszywy host: `Option<Arc<AppState>>`, `spawn_blocking` = `std::thread::spawn`; prawdziwe SQLite w tempdir i `ThumbnailScheduler::with_processor`:
  - `serve_answers_503_until_app_state_exists`;
  - `cached_only_request_for_a_missing_thumbnail_is_404_without_a_render`;
  - `visible_requests_render_lifo_ahead_of_prefetch_and_every_request_is_answered`:
    - przebieg: zajęty render 1, `p=1` dla 2 i 3, widoczne 4 i 5, widoczne 3;
    - kolejność renderów `[1, 3, 5, 4, 2]`;
    - każde z 6 żądań dostaje 200 z właściwymi bajtami przez Publishera;
    - jako jedyny test sięga do wspólnego Publishera procesu.
- Nowy w adapterze: `adapter::thumb_scheme::tests::reply_becomes_an_http_response` (status, nazwy nagłówków jako `tauri::http::header::*`, liczba nagłówków, treść dla 200, 503 i 400).
- Komendy:
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib -- thumb_protocol thumb_scheme a_visible_request`;
  - `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`;
  - `bun run rust:fmt`, `bun run rust:clippy`, `bun run test:backend`, `git diff --check`;
  - `python3 -I -m unittest` w `.claude/skills/audit/scripts`.
- Desktop E2E (jeden spec naraz): `thumbnail-fade` (`MEDIATAGGER_THUMBNAIL_FADE=1`), potem `gallery-browsing` (`MEDIATAGGER_GALLERY_BROWSING=1`).

## Kryteria akceptacji

- `FRAMEWORK_ALLOWLIST` ma 3 wpisy wideo; `allowlist_entries_are_still_needed` i reguła 1 zielone.
- Snapshot wire (komendy, sygnatury, DTO, eventy) bez zmian.
- `rg '\b(tauri|http)\b' src-tauri/src/services/thumb_protocol.rs` pokazuje tylko literały originów i komentarz.
- `--lib` 364 passed, 2 ignored (+4, nic nie zniknęło).
- `git diff src-tauri/src/lib.rs` zmienia tylko rejestrację schematu i jej komentarz.

## Ryzyka

- **Okno 503.** Kod `None → unavailable` przeniesiony dosłownie i pokryty testem rdzenia. Adapter zwracający zawsze `None` przechodzi testy jednostkowe (próba 2b); łapie go tylko E2E.
- **Wspólny Publisher procesu w testach.** Partia jest publikowana hostem żądania, które uruchomiło drenaż. Dlatego tylko jeden test kończy się renderem (`Pending`).
- **Mieszane końce linii** w `thumb_service.rs` i `thumb_scheduler.rs` (patrz notatki).

## Status

done (2026-11-14). Commit czeka na akceptację użytkownika.

## Notatki agentów

### 2026-11-14 — agent etapu 05
- Zrobione:
  - `services/thumb_protocol.rs` bez Tauri: `ThumbReply`, porty `ThumbResponder` (closure) i `ThumbHost` (trait); `serve` i Publisher przeniesione dosłownie (zmienione tylko typy, sprawdzone diffem względem `HEAD`);
  - `adapter/thumb_scheme.rs`: `handle`, `TauriThumbHost`, `into_response`;
  - `lib.rs` zmienia tylko ścieżkę handlera i komentarz;
  - `FRAMEWORK_ALLOWLIST`: 4 → 3 wpisy;
  - dokumentacja, reguły `thumbnails`/`backend-core`, batch audytu B2b, notatki `Z etapu 05` w sekcjach 07 i 08 README.
- Odkryte:
  - LIFO i promocja prefetchu w ogóle nie są w protokole (`thumb_scheduler.rs:547` `push_front`, `:312-321` przesunięcie dołączonego joba, `thumb_service.rs:824`). Protokół tylko mapuje flagi, więc granica przeszła bez ich ruszania;
  - `matching_version_is_immutable_and_stale_url_is_not_cached` porównuje ze stałą `IMMUTABLE`, więc zmiana jej wartości go nie czerwieni (tak było też przed etapem). Wartość pilnuje literał w teście adaptera;
  - **pułapka końców linii:** skrypt mutacyjny w Pythonie (tryb tekstowy, bez `newline=''`) zapisał z powrotem `thumb_service.rs` i `thumb_scheduler.rs` bez `\r`. Pliki przywrócone `git checkout` (nie miały zmian etapu), liczby CRLF wróciły do 423 i 459. Przy mutacjach plików z mieszanymi końcami trzeba używać `open(path, newline='')`;
  - `rustfmt` uruchomiony tylko na dwóch edytowanych plikach, żeby nie dotykać plików CRLF.
- Zostało: commit po akceptacji.
- Testy:
  - `--lib`: przed 360 passed, 2 ignored; po 364 passed, 2 ignored (+4);
  - `cargo test … --lib -- thumb_protocol thumb_scheme a_visible_request` → pass (11);
  - `cargo test … --test architecture_boundaries` → pass (23 passed, 1 ignored);
  - próby mutacyjne (12), każda cofnięta (pliki LF sprawdzone `md5sum`, pliki CRLF przywrócone z gita):
    - 1. utrata LIFO (`push_front` → `push_back` dla Interactive w `thumb_scheduler`) → czerwone: test kolejności `serving` oraz `interactive_jobs_run_newest_first_and_ahead_of_high` i `visible_requests_for_queued_prefetches_render_before_the_scrolled_past_backlog` w schedulerze;
    - 2a. brak 503 przed `AppState` (`else` w `serve` → `not_found`) → `serve_answers_503_until_app_state_exists` czerwony;
    - 2b. `TauriThumbHost::app_state` zawsze `None` → wszystkie 364 testy `--lib` zielone (zgodnie z oczekiwaniem; łapie to tylko E2E);
    - 3a. inna wartość `IMMUTABLE` → `reply_becomes_an_http_response` czerwony (test rdzenia nie, patrz „Odkryte”);
    - 3b. `==` → `!=` w `image_response` → oba testy wersji i test adaptera czerwone;
    - 4a. `prefetch: !request.prefetch` w `serve` → test kolejności `serving` czerwony;
    - 4b. `if !mode.prefetch` w `thumb_service.rs:824` → `a_visible_request_moves_a_queued_prefetch_ahead…` i test kolejności `serving` czerwone;
    - 5. `cached_only: false` w `serve` → test `c=1` czerwony;
    - 6a. `into_response` bez nagłówków → test adaptera czerwony;
    - 6b. `into_response` ze stałym statusem 200 → test adaptera czerwony;
    - 7a. `use tauri::http::header;` w `thumb_protocol.rs` → `core_does_not_use_the_gui_runtime` (`src/services/thumb_protocol.rs:23: rule 1: tauri`);
    - 7b. ponowny wpis allowlisty → `allowlist_entries_are_still_needed` („remove it from FRAMEWORK_ALLOWLIST (stage 05 done)”);
  - `python3 -I -m unittest` (skrypty audytu) → pass (31);
  - `bun run rust:fmt` → pass; `bun run rust:clippy` → pass;
  - `bun run test:backend` → pass (lib 364, `architecture_boundaries` 23, pozostałe cele zielone);
  - `git diff --check` → pass;
  - desktop E2E (prywatny Xvfb, tymczasowe `XDG_*`, jeden spec naraz):
    - `thumbnail-fade` (`MEDIATAGGER_THUMBNAIL_FADE=1`) → 2 passing;
    - `gallery-browsing` (`MEDIATAGGER_GALLERY_BROWSING=1`) → 2 passing.
