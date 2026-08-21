# Etap 00 - Uporządkowanie bieżącego stanu roboczego

## Metryka

- Priorytet: wysoki
- Status: Ukończony
- Zależności: brak
- Następny etap: 01

## Cel

Rozliczyć istniejące niezacommitowane zmiany przed rozpoczęciem napraw integralności danych. Zachować poprawę asynchronicznego streamingu wideo, ale przywrócić kontrolę zużycia zasobów i upewnić się, że dokumentacja opisuje rzeczywisty stan kodu.

## Potwierdzone ryzyka

- `src-tauri/src/services/media_server.rs` tworzy osobny task dla każdego połączenia bez limitu aktywnych streamów.
- Nowa implementacja nie ma timeoutu odczytu nagłówków, bezczynności ani zapisu.
- Pojedynczy błąd `listener.accept()` kończy pętlę serwera bez oznaczenia stanu jako niedostępny.
- Zmiany obejmują również cache stron, nawigację lightboxa, obsługę błędów mediów, Cargo dependencies, testy E2E i dokumentację.
- Główny plan historyczny nie odnotowuje rozpoczęcia tych prac.

## Zadania

- [x] Odczytać pełny diff i sklasyfikować każdą zmianę jako ukończoną, wymagającą naprawy albo niezwiązaną z planem integralności.
- [x] Zachować asynchroniczny, backpressure-aware streaming przez Hyper/Tokio.
- [x] Dodać jawny limit aktywnych połączeń lub streamów bez powrotu do blokującej kolejki workerów.
- [x] Dodać timeout nagłówków i bezczynności połączenia oraz kontrolowany timeout zapisu.
- [x] Obsłużyć przejściowe błędy `accept()` z retry/backoff; trwała awaria ma być obserwowalna przez stan serwera.
- [x] Potwierdzić poprawne zamknięcie tasków po shutdownie aplikacji.
- [x] Sprawdzić, czy zależności Cargo mają minimalny wymagany zestaw feature flags.
- [x] Zsynchronizować dokumentację media servera i lightboxa z ostatecznym zachowaniem.
- [x] Zaktualizować `README.md` tego katalogu oraz dziennik poniżej.

## Kryteria odbioru

- Liczba równoległych połączeń i czas życia bezczynnego połączenia są ograniczone.
- Przejściowy błąd listenera nie wyłącza bezpowrotnie streamingu.
- Szybka zmiana filmów nadal wybiera najnowszy zasób, a wolny klient nie blokuje innych odczytów.
- Cargo manifest, lockfile, kod i dokumentacja opisują ten sam stos wykonawczy.
- Wszystkie istniejące zmiany robocze mają jawnie określony status.

## Planowana weryfikacja

- Statyczny przegląd diffu i zależności.
- Testy jednostkowe zakresów HTTP, shutdownu, limitu połączeń i timeoutów po uzyskaniu osobnej zgody.
- Desktop E2E szybkiego przełączania i ciągłego odtwarzania po uzyskaniu osobnej zgody.

## Dziennik

### 2026-08-21 - sesja implementacyjna

- Status po sesji: Ukończony
- Zmienione pliki: `src-tauri/src/services/media_server.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, zmiany lightboxa, selekcji, cache stron i ich zastane testy, desktop E2E, dokumentacja architektury i cały katalog `docs/integration_fix_plans/`
- Wdrożone zachowanie: asynchroniczny streaming Hyper/Tokio z limitem 32 połączeń, timeoutem nagłówków 5 s, przygotowania odpowiedzi 10 s i zablokowanego zapisu 30 s; bez keep-alive; retry `accept()` z backoffem do 1 s i stanem trwałej awarii po 8 kolejnych błędach; abort i opróżnienie tasków przy shutdownie; współdzielenie Promise ładowanej strony; nawigacja lightboxa zgodna z ostatnim szybkim poleceniem i izolacja błędów kolejnych aktywacji mediów
- Decyzje i odchylenia: cały zastany diff aplikacji i dokumentacji sklasyfikowano jako spójny z Etapem 00; `skills-lock.json` sklasyfikowano jako niezwiązany i pozostawiono poza commitem; pliki planu włączono jako nadrzędne źródło stanu; odrzucone połączenie ponad limit jest zamykane bez blokowania pętli listenera
- Migracje i kompatybilność: brak migracji danych i zmian IPC; HTTP pozostaje zgodne dla GET, HEAD i pojedynczych zakresów bajtowych, ale każde połączenie obsługuje dokładnie jedno żądanie
- Utworzone lub zmienione testy: zastany diff zawiera testy Rust, Vitest i desktop E2E opisujące streaming, szybkie przejścia i współdzielenie ładowania strony; w tej sesji ich nie zmieniano z powodu braku osobnej zgody
- Uruchomione sprawdzenia: `git diff --check`; próba `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` nie uruchomiła formatowania, ponieważ `cargo` nie jest dostępne w środowisku
- Niewykonane sprawdzenia i ryzyka: zgodnie z wiążącym zakazem nie uruchomiono testów, buildów ani E2E; brak dostępnego Cargo uniemożliwił także automatyczne sprawdzenie formatowania Rust; limit, timeouty, retry i shutdown wymagają uruchomienia istniejących testów po osobnej zgodzie
- Następny dokładny krok: w nowej, czystej sesji rozpocząć Etap 01, oznaczyć go jako `W toku` i prześledzić aktualny workflow usuwania plików oraz rozwiązywania duplikatów
