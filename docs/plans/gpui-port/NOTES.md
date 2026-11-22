# Dziennik przekazań

Jedna linia na wpis, najnowsze na dole. Szczegóły trafiają do sekcji `## Notatki agentów` w pliku epiku. Linie dopisuje `port_state.py set-status`.

Format: `YYYY-MM-DD | epik | rola | status | jedno zdanie | link do epiku`

---

2026-11-20 | E00 | orkiestrator | in progress | workflow start od kroku implement | [E00](E00-spike-skeleton.md)
2026-11-20 | E00 | workflow | done | Epik E00 ukończony: osobny workspace GPUI z oknem powitalnym, przegląd czysty, bramka test:all zaliczona. | [E00](E00-spike-skeleton.md)
2026-11-20 | E01 | orkiestrator | in progress | workflow start od kroku implement | [E01](E01-compare-harness.md)
2026-11-20 | E01 | workflow | done | E01 ukończony: harness porównawczy Tauri i GPUI działa, review czyste, bramka test:all zielona. | [E01](E01-compare-harness.md)
2026-11-20 | E02 | orkiestrator | in progress | workflow start od kroku implement | [E02](E02-client-backend-port.md)
2026-11-20 | E02 | workflow | done | E02 ukończony: port Backend, wykonawca i potok miniatur w crate klienta, przegląd czysty, bramka zielona. | [E02](E02-client-backend-port.md)
2026-11-21 | E03 | orkiestrator | in progress | workflow start od kroku implement | [E03](E03-client-i18n-theme-prefs.md)
2026-11-21 | E03 | workflow | done | Epik E03 zakończony: i18n, tokeny motywu i preferencje UI gotowe, przegląd czysty, brama zielona. | [E03](E03-client-i18n-theme-prefs.md)
2026-11-21 | E04 | orkiestrator | in progress | workflow start od kroku implement | [E04](E04-text-input.md)
2026-11-21 | E04 | workflow | done | E04 gotowy: pole tekstowe (model w crate klienta i element GPUI) z testami, przegląd czysty po 3 rundach, bramka test:all zaliczona. | [E04](E04-text-input.md)
2026-11-21 | E05 | orkiestrator | in progress | workflow start od kroku implement | [E05](E05-gpui-primitives-overlays.md)
2026-11-21 | E05 | workflow | done | E05 zakończony: prymitywy UI i warstwa nakładek w GPUI gotowe, przegląd czysty po 2 rundach, bramka zaliczona. | [E05](E05-gpui-primitives-overlays.md)
2026-11-21 | E06 | orkiestrator | in progress | workflow start od kroku implement | [E06](E06-gpui-app-shell.md)
2026-11-21 | E06 | workflow | done | Epik E06 ukończony: runtime, okno i powłoka gotowe, przegląd czysty, brama test:all zaliczona; nie sprawdzono na prawdziwym ekranie. | [E06](E06-gpui-app-shell.md)
2026-11-21 | E07 | orkiestrator | in progress | workflow start od kroku implement | [E07](E07-client-query-model.md)
2026-11-21 | E07 | workflow | done | E07 zakończony: model zapytań i cache stron w kliencie, testy i bramka zielone, przegląd czysty. | [E07](E07-client-query-model.md)
2026-11-21 | E08 | orkiestrator | in progress | workflow start od kroku implement | [E08](E08-client-metadata.md)
2026-11-21 | E08 | workflow | done | E08 ukończony: model biblioteki, stan tagów, mutacje i słownik w kliencie, przegląd czysty, bramka zaliczona. | [E08](E08-client-metadata.md)
2026-11-21 | E09 | orkiestrator | in progress | workflow start od kroku implement | [E09](E09-gpui-gallery-grid.md)
2026-11-21 | E09 | workflow | done | E09 ukończony: siatka galerii i miniatury gotowe, przegląd czysty, bramka test:all przeszła. | [E09](E09-gpui-gallery-grid.md)
2026-11-22 | E10 | orkiestrator | in progress | workflow start od kroku implement | [E10](E10-gpui-scrollbar-timeline.md)
2026-11-22 | E10 | workflow | done | E10 gotowe: pasek pozycji i scrubber osi czasu w GPUI, przegląd czysty, bramka zielona; parytet gallery-default bez paska z powodu seeda. | [E10](E10-gpui-scrollbar-timeline.md)
2026-11-22 | E11 | orkiestrator | in progress | workflow start od kroku implement | [E11](E11-selection.md)
2026-11-22 | E11 | workflow | blocked | Zablokowany: test:all pada w audit:rust na RUSTSEC-2026-0592 (url), niezwiązanym z epikiem; do poprawy zostały 2 testy GPUI (RectangleCancel przy zamknięciu galerii). | [E11](E11-selection.md)
2026-11-22 | E11 | orkiestrator | in progress | workflow start od kroku gate | [E11](E11-selection.md)
2026-11-22 | E11 | workflow | done | Epik ukończony: bramka test:all przeszła w 1 rundzie po wcześniejszej blokadzie. | [E11](E11-selection.md)
2026-11-22 | E12 | orkiestrator | in progress | workflow start od kroku implement | [E12](E12-search.md)
2026-11-22 | E12 | workflow | done | E12 zakończony: wyszukiwarka, gramatyka filtrów i podpowiedzi gotowe, przegląd czysty, gate zielony; parity compare:ui nieuruchomione poprawnie. | [E12](E12-search.md)
2026-11-22 | E13 | orkiestrator | in progress | workflow start od kroku implement | [E13](E13-tag-list-editor.md)
2026-11-22 | E13 | workflow | done | E13 gotowy: modal listy tagów i TagEditor w GPUI, testy, parytet i brama przeszły; zostały trzy niepewne uwagi wydajnościowe P3 do pomiaru. | [E13](E13-tag-list-editor.md)
2026-11-22 | E14 | orkiestrator | in progress | workflow start od kroku implement | [E14](E14-bulk-dock.md)
2026-11-22 | E14 | workflow | done | E14 ukończony: dok akcji zbiorczych i kolejność grup, kontrola, recenzja i bramka zielone; otwarta tylko różnica rasteryzacji fontów w parytecie. | [E14](E14-bulk-dock.md)
2026-11-22 | E15 | orkiestrator | in progress | workflow start od kroku implement | [E15](E15-client-lightbox-model.md)
2026-11-22 | E15 | workflow | done | Model lightboxa gotowy i osadzony w AppRuntime, testy i bramka przechodzą, bez otwartych ustaleń przeglądu. | [E15](E15-client-lightbox-model.md)
2026-11-22 | E14 | orkiestrator | done | Kolejność seeda `control` przypięta: `pinControlSeedOrder` w `e2e/compare/seed.js` nadaje plikom różne sekundy `mtime` według nazwy (wspólna reguła dla `seedMedia` i `app:control start`), więc galeria i plansza kolejności są takie same w Tauri i GPUI w każdym przebiegu. | [E14](E14-bulk-dock.md)
