# Odtworzenie projektu po reinstalacji Windows

Projekt jest przygotowany dla 64-bitowego Windows. Katalog zawiera kod źródłowy,
testy, assety oraz lockfile'e zależności. Nie zawiera historii Git, pobranych
zależności ani wyników kompilacji.

## Wymagane narzędzia

1. Node.js w wersji zgodnej z `package.json`: `>=20.19 <21` lub `>=22.12`.
2. Bun.
3. Stabilny Rust z toolchainem `x86_64-pc-windows-msvc` oraz Cargo.
4. Microsoft C++ Build Tools z Windows SDK.
5. Microsoft Edge WebView2 Runtime.

Pełna konfiguracja środowiska jest opisana w
`docs/development/setup-and-build.md`.

## Instalacja i uruchomienie

W PowerShell, z katalogu głównego projektu:

```powershell
bun install --frozen-lockfile
bun run build
bun run tauri:dev
```

Pierwsze polecenie odtwarza zależności JavaScript z `bun.lock`. Cargo pobierze
brakujące zależności Rust przy pierwszym buildzie na podstawie
`src-tauri/Cargo.lock`.

## Testy

```powershell
bun run test
bun run test:backend
```

Testy desktopowe E2E wymagają dodatkowo `tauri-driver` oraz EdgeDrivera zgodnego
z wersją zainstalowanej przeglądarki Microsoft Edge. Szczegóły znajdują się w
`docs/development/testing.md`.

## Dane aplikacji

Ten katalog nie zawiera danych użytkownika z `%APPDATA%`, w tym bazy biblioteki,
tagów ani wygenerowanych miniaturek. Jeżeli te dane mają zostać zachowane,
trzeba przenieść je oddzielnie.
