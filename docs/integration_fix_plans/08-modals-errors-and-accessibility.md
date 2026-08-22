# Etap 08 - Błędy, modale i dostępność

## Metryka

- Priorytet: średni
- Status: Ukończony
- Zależności: Etapy 04 i 05
- Następny etap: 09

## Cel

Zapewnić kontrolowaną obsługę błędów oraz jeden stos warstw UI, w którym Escape, fokus i tło są zarządzane wyłącznie przez najwyższy aktywny dialog.

## Potwierdzone ryzyka

- `refresh` i `loadPage` nie wystawiają stanu błędu ani Retry.
- Wywołania przez `void` mogą tworzyć nieobsłużone rejection.
- Zakres wirtualny może zostać oznaczony jako obsłużony przed sukcesem strony.
- `UiModal`, settings i lightbox rejestrują niezależne listenery `window.keydown`.
- Jeden Escape może zamknąć potwierdzenie, resolver i settings albo cały lightbox.
- Lightbox nie ma kompletnej semantyki dialogu i focus trap.
- Zamknięte panele mogą pozostawiać focusowalne kontrolki.
- Combobox tagów i reorder grup nie mają pełnej obsługi klawiatury/ARIA.
- Brak Error Boundary dla lazy chunks.

## Zadania

- [x] Dodać jawny stan błędu i Retry dla startu query, stron i danych szczegółowych.
- [x] Obsługiwać każdą celowo porzuconą Promise przez kontrolowany error path.
- [x] Dodać Error Boundary dla lazy-loaded settings, bulk i lightbox.
- [x] Wprowadzić wspólny manager stosu modali/warstw.
- [x] Pozwolić tylko najwyższej warstwie obsłużyć Escape i backdrop.
- [x] Dodać focus trap, początkowy fokus i przywracanie fokusu triggera.
- [x] Oznaczać tło jako inert/aria-hidden podczas aktywnego dialogu.
- [x] Ujednolicić `role=dialog`, `aria-modal`, nazwę i opis dialogów.
- [x] Usuwać zamknięte panele z tab order.
- [x] Dokończyć semantykę combobox/listbox i keyboard reorder.
- [x] Dodać klawiaturowy odpowiednik double-click dla wykluczania tagu.
- [x] Respektować `prefers-reduced-motion`.

## Kryteria odbioru

- Jeden Escape zamyka wyłącznie najwyższą warstwę.
- Fokus nie wychodzi poza aktywny dialog i wraca do logicznego triggera.
- Błąd query lub lazy chunk nie pozostawia pustego ekranu bez Retry.
- Główne przepływy lightboxa, tagów i grup są dostępne z klawiatury.
- Ukryte kontrolki nie pozostają focusowalne.

## Planowana weryfikacja

- Zagnieżdżone dialogi, Escape, backdrop i focus restoration.
- Query/page rejection, retry i lazy chunk failure.
- Testy Testing Library i ręczna walidacja desktop/mobile po uzyskaniu osobnej zgody.

## Dziennik

### 2026-08-22 - sesja implementacyjna

- Status po sesji: Ukończony.
- Zmienione pliki: warstwa aplikacji i kontrolery selekcji, komponenty UI/lightboxa/settings/search/bulk/tag-list, testy frontendowe, style globalne, wszystkie locale oraz dokumenty planu.
- Wdrożone zachowanie: kontrolowane błędy z Retry, Error Boundary dla lazy chunks, jeden stos warstw z obsługą Escape/backdrop/fokusu/tła, modalny lightbox także podczas ładowania, semantyka combobox/listbox, klawiaturowy reorder i wykluczanie tagów oraz reduced motion.
- Decyzje i odchylenia: settings rejestruje warstwę poza lazy komponentem; fallback i właściwy lightbox używają wspólnego logicznego celu przywracania fokusu; błąd pełnych szczegółów zasobu jest niezależny od dostępności autorytatywnych tagów; sugestie mają prosty indeks zapasowy, gdy Fuse nie może się załadować.
- Migracje i kompatybilność: brak migracji danych i zmian IPC.
- Utworzone lub zmienione testy: regresje dla query/page Retry, lazy failure, stosu dialogów, fokusu, zagnieżdżonego potwierdzenia, błędów szczegółów, combobox/listbox, reorder, wykluczania tagów, ukrytych paneli i reduced motion.
- Uruchomione sprawdzenia: `bun run build`; pełne `bun run test:all` z Node 22 i izolowanym `CARGO_TARGET_DIR` - frontend 348/348, Rust 126 + 1 + 5, desktop E2E 13/13 w 2 plikach spec; `git diff --check` bez błędów.
- Niewykonane sprawdzenia i ryzyka: nie wykonano ręcznego audytu czytnikiem ekranu, kontrastu, zoomu ani pełnego przejścia wyłącznie klawiaturą na każdym wspieranym systemie.
- Następny dokładny krok: rozpocząć Etap 09 - Lokalizacja i quality gates.
