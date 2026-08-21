# Etap 10 - Końcowa weryfikacja integralności

## Metryka

- Priorytet: wysoki
- Status: Oczekuje
- Zależności: Etapy 00-09
- Następny etap: brak

## Cel

Potwierdzić, że wszystkie wcześniejsze etapy tworzą jeden spójny system, nie przywróciły wcześniejszych wyścigów i mają kompletną dokumentację zachowania oraz recovery.

## Zadania audytowe

- [ ] Sprawdzić wszystkie kryteria odbioru Etapów 00-09.
- [ ] Porównać typy TypeScript z modelami Rust i rzeczywistą serializacją IPC.
- [ ] Porównać wrappery `src/api.ts` z listą `generate_handler!` i usunąć niepotrzebne legacy endpoints.
- [ ] Sprawdzić kolejność maintenance, scan i thumb locków we wszystkich workflow.
- [ ] Sprawdzić atomowość mutacji i revision dla scan, delete, rename, tag, favorite, group, CSV i restore.
- [ ] Sprawdzić confinement każdej ścieżki używanej do usuwania lub nadpisywania pliku.
- [ ] Sprawdzić recovery po każdym commit point operacji filesystem plus DB.
- [ ] Sprawdzić invalidation query, summary, details, selection i thumbnail cache.
- [ ] Sprawdzić limity ZIP, obrazów, kolejek, połączeń i timeoutów.
- [ ] Potwierdzić zachowanie basename-only i fan-out CSV.
- [ ] Potwierdzić kompletność locale, placeholderów i dostępności modalnej.
- [ ] Porównać dokumentację architektury, IPC, bazy, backupu i testów z kodem.
- [ ] Zaktualizować trwałe reguły w `AGENTS.md` tylko wtedy, gdy są rzeczywiście ogólnoprojektowe.
- [ ] Zamknąć albo jawnie zaakceptować każde pozostałe ryzyko.

## Planowana walidacja automatyczna

Poniższe komendy można uruchomić wyłącznie po osobnym, wyraźnym poleceniu:

```bash
bun install --frozen-lockfile
bun run build
bun run test
bun run test:backend
bun run test:e2e:tauri
```

Dodatkowo należy uruchomić zaakceptowane w Etapie 09 kontrole lint, format, locale, Rust fmt/clippy i advisory. Dla pakietu Windows potrzebny jest osobny smoke test zainstalowanego MSI/NSIS i sidecarów. Dla Linux release należy użyć udokumentowanego workflow Docker.

## Kryteria odbioru

- Każdy etap ma status, daty, dziennik i rozliczone kryteria.
- Nie istnieje mutacja widoczna dla query bez atomowego revision lub jawnie udokumentowanej przyczyny.
- Żadna niezaufana ścieżka nie może sterować usuwaniem poza dozwolonym rootem.
- Backup i restore mają potwierdzoną spójność oraz recovery.
- Frontend i backend zgadzają się na wszystkich publicznych kontraktach.
- Wszystkie autoryzowane bramki weryfikacyjne przechodzą albo mają opisany blocker.
- `README.md` wskazuje brak kolejnego etapu i końcowy zestaw zaakceptowanych ryzyk.

## Dziennik

- Brak wpisów.
