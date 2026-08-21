# Etap 08 - Błędy, modale i dostępność

## Metryka

- Priorytet: średni
- Status: Oczekuje
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

- [ ] Dodać jawny stan błędu i Retry dla startu query, stron i danych szczegółowych.
- [ ] Obsługiwać każdą celowo porzuconą Promise przez kontrolowany error path.
- [ ] Dodać Error Boundary dla lazy-loaded settings, bulk i lightbox.
- [ ] Wprowadzić wspólny manager stosu modali/warstw.
- [ ] Pozwolić tylko najwyższej warstwie obsłużyć Escape i backdrop.
- [ ] Dodać focus trap, początkowy fokus i przywracanie fokusu triggera.
- [ ] Oznaczać tło jako inert/aria-hidden podczas aktywnego dialogu.
- [ ] Ujednolicić `role=dialog`, `aria-modal`, nazwę i opis dialogów.
- [ ] Usuwać zamknięte panele z tab order.
- [ ] Dokończyć semantykę combobox/listbox i keyboard reorder.
- [ ] Dodać klawiaturowy odpowiednik double-click dla wykluczania tagu.
- [ ] Respektować `prefers-reduced-motion`.

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

- Brak wpisów.
