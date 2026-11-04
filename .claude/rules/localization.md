---
paths:
  - "src/i18n/**"
  - "scripts/*locale*"
---

# Localization

Read `docs/development/localization.md`.

- 11 languages: en, pl, fr, de, it, es, ru, zh, ja, ko, cs. `src/i18n/locales/en.json` is canonical.
- A language is registered in `APP_LANGUAGES` and `APP_LANGUAGE_NATIVE_LABELS` (`src/i18n/languages.ts`) and in the `resources` imports in `src/i18n/core.ts`. All three must agree.
- New key: add it to `en.json`, then make the same structural edit in all 10 other files, with the same path, value type, and exact `{{placeholder}}` set. Extra keys are errors. Don't rely on the English fallback.
- `lightbox.deleteConfirm.confirmWord` is the word users type to confirm a delete. Each language has a reviewed word, so don't machine-translate it.
- Validate: `bun run locale:check`, `bun run test:locale-tools`, plus the shell and lightbox tests listed in the doc. `bun run locale:generate` uses the network and fills only missing keys.
