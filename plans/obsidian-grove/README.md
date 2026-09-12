# Obsidian Grove

Redesign Tagrove według makiet `docs/frontend/{gallery,sorting_group,lightbox,settings}.png`. Plan użytkownika jest nadrzędny wobec propozycji `model_DESIGN.md`. Starszy `docs/ui-redesign-plan.md` pozostaje historią poprzedniej pracy. Makiety wejściowe i `model_DESIGN.md` są lokalnymi materiałami referencyjnymi, poza commitem; makieta ustawień zawiera prywatne ścieżki.

## Ustalenia

- Bieżący checkout, bez worktree. IPC, baza, CSV, backup i preferencje pozostają zgodne.
- Pełny ciemny i jasny motyw. Logo SVG odtwarza `new_logo.png`, bez tła i poświaty.
- Inter 13 px, tekst pomocniczy 12 px, nagłówki 18/24 px; JetBrains Mono dla ścieżek i metadanych. Fonty lokalne z licencjami i fallbackami.
- Dark: #141618 / #202224 / #232528 / #2c2e31, akcent #10b981 z ciemnym tekstem. Light: #f4f6f5 / #ffffff / #e8eeeb / #ccd7d0, tekst #17241e / #52635a, akcent #087f5b z białym tekstem.
- Tagi autosave, grupy osobne Apply. Bez zbiorczego Save Changes.
- Bulk 360 px od 1000 px, pod galerią poniżej progu. Suwak 140–340 px, domyślnie 188 px, obecny krok.
- Ustawienia mają własny nagłówek i Powrót. Lightbox blokuje galerię, panel 340 px i drawer poniżej 768 px.
- Jedyna nowa funkcja: Ctrl+K / Cmd+K ustawia fokus w wyszukiwaniu, bez zmiany tekstu ani wysłania zapytania, poza modalami.
- Bez EXIF, ostatnich tagów, edytowalnych chipów zapytań i dodatkowych akcji kafli.

## Etap 0 i punkt odniesienia

Uruchomiono prawdziwy Tauri na prywatnym Xvfb, z siedmioma syntetycznymi mediami i tymczasową bazą. Doctor potwierdził profil i root. Sesję zamknięto przez controller.

Dowody lokalne: `artifacts/app-control/cb7eb31f856474257ea636ffa4859c12/`. `screen-0001.png`: galeria, `screen-0002.png`: ustawienia, `screen-0003.png`: lightbox. `actions.jsonl` i `session.json` dokumentują działania i izolację. Zrzuty nie trafiają do publicznego repozytorium.

Punkt odniesienia obejmuje 1440×900 EN dark. Porównanie wydajności dużej biblioteki wymaga osobnego pomiaru, nie wynika z tych zrzutów.

## Kolejność

- [01. Wspólny wygląd i logo](01.md)
- [02. Nagłówek, wyszukiwanie i galeria](02.md)
- [03. Akcje zbiorcze i lista tagów](03.md)
- [04. Modal sortowania grupy](04.md)
- [05. Lightbox i natywne wideo](05.md)
- [06. Ustawienia i pozostałe dialogi](06.md)
- [07. Odbiór całego UI i dokumentacja](07.md)

## Checklista zachowania funkcji

- [x] Search draft/applied, podpowiedzi, walidacja, reset, rodzaje, ulubione, lista tagów.
- [x] Suwak, resize, cache, wirtualizacja, grupowe obramowania, loading/error/empty/retry.
- [x] Bulk click/Ctrl/Shift/prostokąt/autoscroll, czyszczenie, dane poza załadowanymi stronami.
- [x] Single/multi tagi, popularne tagi, ulubione, grupa/UUID/unassign, oba sortowania.
- [x] Sort drag/gaps/edges, cztery strzałki, fokus, szkic/save/cancel/failure/partial.
- [x] Lightbox fit/zoom/pan, nawigacja globalna, autosave, grupy, szczegóły, kopiowanie, delete.
- [x] Obraz/GIF/video, panel/drawer, fullscreen, focus i teardown odtwarzania.
- [x] Sześć sekcji ustawień, scan/Stop/thumbnails/diagnostyka, statusy poza widokiem.
- [x] CSV/backup/restore, mapowanie katalogów, resolver/UUID/kolejka/save/cancel/delete.
- [x] PL/EN i 11 kompletów etykiet, oba motywy, wąskie widoki, klawiatura/reduced motion.

## Odbiór

Ukończono etapy 0–7. Osiem pakietów desktop obejmuje 63 różne scenariusze, wszystkie zakończone PASS w poniższych przebiegach. Po ostatniej korekcie tytułów ponownie przeszły frontend, sortowanie, ekran light 1000×720 z dialogami oraz produkcyjny build. Logi są w `artifacts/obsidian-grove/`.

| Sprawdzenie | Wynik | Dowód |
| --- | --- | --- |
| Build produkcyjnego frontendu | PASS, TypeScript i Vite | [build.log](../../artifacts/obsidian-grove/build.log) |
| Kontrola po ostatniej korekcie tytułów | 1/1 desktop UI PASS | [ui-titles-accepted.log](../../artifacts/obsidian-grove/ui-titles-accepted.log) |
| Frontend | 80 plików, 584 testy PASS | [frontend.log](../../artifacts/obsidian-grove/frontend.log) |
| Lint i 11 języków | PASS | [lint.log](../../artifacts/obsidian-grove/lint.log), [locales.log](../../artifacts/obsidian-grove/locales.log) |
| Rust check/fmt | PASS | [rust-check.log](../../artifacts/obsidian-grove/rust-check.log), [rustfmt.log](../../artifacts/obsidian-grove/rustfmt.log) |
| Rust wideo | 17 testów PASS | [rust-video.log](../../artifacts/obsidian-grove/rust-video.log) |
| Desktop reduced motion | 25/25 PASS | [desktop-final.log](../../artifacts/obsidian-grove/desktop-final.log), pakiet animations |
| Desktop duża galeria | 3/3 PASS | ten sam log, pakiet gallery.performance |
| Desktop zaznaczenie | 5/5 PASS | ten sam log, pakiet gallery-selection |
| Desktop UI | 9/9 PASS, oba motywy, PL/EN, drawer i fokus przy GTK | [ui-accepted.log](../../artifacts/obsidian-grove/ui-accepted.log) |
| Desktop regresje i przepływy | 3/3 + 13/13 PASS | [workflows-accepted.log](../../artifacts/obsidian-grove/workflows-accepted.log) |
| Desktop natywne wideo | 4/4 PASS, 30 cykli z kontrolą obrazu | [video-accepted.log](../../artifacts/obsidian-grove/video-accepted.log) |
| Desktop sortowanie | 1/1 PASS | [group-visual-accepted.log](../../artifacts/obsidian-grove/group-visual-accepted.log) |

`desktop-final.log` zawiera także wcześniejszy błąd przygotowania zrzutu sortowania i urywa się podczas UI po przerwaniu sesji. Nie jest dowodem zaliczenia wszystkich ośmiu pakietów. Poszczególne wyniki powyżej odnoszą się wyłącznie do zakończonych pakietów.

## Granice odbioru

- Desktop sprawdzono na Linuksie, na prywatnym ekranie X11/Xvfb z działającym GTK i OpenGL. Nie wykonano odbioru Wayland ani pakietów instalacyjnych.
- Systemowe okna wyboru plików i mapowania katalogów zachowują wygląd systemu. W aplikacji nowy motyw obejmuje potwierdzenia i resolver. CSV i backup wykonano przez prawdziwe IPC; nie sterowano systemowym wyborem pliku ani mapowaniem katalogów przez natywny picker.
- PL i EN sprawdzono wizualnie. Pozostałe dziewięć języków przeszło walidację kluczy i placeholderów.
- Pomiar wirtualizacji porównuje zamontowane kafle i zakresy przewijania. Nie jest pomiarem FPS ani czasu dekodowania.

## Porównanie wirtualizacji

Uruchomiono rzeczywisty TanStack Virtual ze źródłem hooka z HEAD i nowym hookiem, przy tej samej geometrii jsdom i 2051 elementach. Próby obejmowały początek, 10%, 50%, 90% i koniec przewijania. Liczby zamontowanych kafli i zakresy indeksów były identyczne. Maksima wyniosły 91, 52, 24 i 12 dla szerokości siatki 1400, 960, 560 i 280 px. Nowy odstęp zmienia całkowitą wysokość siatki zgodnie z geometrią.

Dowód: `artifacts/obsidian-grove/obsidian-comparison.json`, log i skrypt obok. To porównanie działania wirtualizera, nie benchmark FPS ani czasów dekodowania. Dodatkowy desktop E2E potwierdził pełne zaznaczenie 2051 elementów przez niezaładowane i usuwane z cache strony, przy mniej niż 200 kaflach w DOM.


## Końcowe dowody wizualne

- [Galeria dark EN 1440×900](../../artifacts/ui-redesign/2026-09-12T19-31-57-510Z/dark-1440-gallery.png), [galeria light PL 1000×720](../../artifacts/ui-redesign/2026-09-12T19-31-57-510Z/pl-light-1000-gallery.png).
- [Lightbox PL dark](../../artifacts/ui-redesign/2026-09-12T19-31-57-510Z/pl-dark-1440-lightbox.png), [wideo z potwierdzeniem usunięcia](../../artifacts/ui-redesign/2026-09-12T19-31-57-510Z/native-video-delete-focus.png).
- [Sortowanie 96 elementów, light PL po resize](../../artifacts/group-order/2026-09-12T19-40-44-072Z/light-pl-wide.png), [wąskie sortowanie](../../artifacts/group-order/2026-09-12T19-40-44-072Z/light-pl-narrow.png).
- [Ustawienia light PL](../../artifacts/ui-redesign/2026-09-12T19-31-57-510Z/pl-light-1000-settings.png), [potwierdzenie z końcowym rozmiarem tytułu](../../artifacts/ui-redesign/2026-09-12T19-41-06-562Z/light-1000-clear-confirmation.png).

Dowody pozostają lokalnie pod ignorowanym `artifacts/`; używają syntetycznych mediów. Bieżący checkout zawiera całą implementację. Nie utworzono worktree.

## Ślad decyzji

[decisions.tsv](decisions.tsv) zachowuje historię poprawek i ponownych sprawdzeń. Wcześniejsze logi `/tmp` zniknęły między sesjami; późniejsze wpisy wskazują trwałe ponowne sprawdzenia. Nie odtwarzano historycznych logów z nowych wyników. Audyt między modelami korzysta z kodu i artefaktów, ponieważ w workspace nie ma katalogu `agent-transcripts/`.

[Końcowy audyt gpt-5.5](../../artifacts/obsidian-grove/audit.txt): brak nierozwiązanych uwag.

## Korekta kontrastu tabów

Aktywny filtr medium ma pełne zielone tło i kontrastowy tekst. Grupa ma tło i ramkę, a hover dotyczy nieaktywnych filtrów. Sprawdzono zrzuty dark/light PL przy 1000 px oraz dark PL przy 320 px. Testy komponentu: 4/4 PASS. Desktop: 8/8 PASS, dark/light × PL/EN × 320/1000 px. Logi lokalne: `artifacts/obsidian-grove/tab-contrast-unit.log` i `tab-contrast-desktop.log`.
