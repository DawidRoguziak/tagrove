# Localization

MediaTagger uses i18next and react-i18next for frontend localization. English is the fallback language and the canonical translation tree. Locale data is bundled with the frontend; changing language does not require a network request.

## Supported languages and sources of truth

The application supports these 11 languages, in the order shown in the settings selector:

| Code | Native selector label | Resource file | Destructive confirmation word |
| --- | --- | --- | --- |
| `en` | English | `src/i18n/locales/en.json` | `Yes` |
| `pl` | Polski | `src/i18n/locales/pl.json` | `Tak` |
| `fr` | Français | `src/i18n/locales/fr.json` | `Oui` |
| `de` | Deutsch | `src/i18n/locales/de.json` | `Ja` |
| `it` | Italiano | `src/i18n/locales/it.json` | `Sì` |
| `es` | Español | `src/i18n/locales/es.json` | `Sí` |
| `ru` | Русский | `src/i18n/locales/ru.json` | `Да` |
| `zh` | 中文（简体） | `src/i18n/locales/zh.json` | `是` |
| `ja` | 日本語 | `src/i18n/locales/ja.json` | `はい` |
| `ko` | 한국어 | `src/i18n/locales/ko.json` | `예` |
| `cs` | Čeština | `src/i18n/locales/cs.json` | `Ano` |

There is deliberately more than one source that must stay synchronized:

- `APP_LANGUAGES` in `src/i18n/languages.ts` is the ordered runtime allowlist. It controls selector order and language normalization. `AppLanguage` is derived from this tuple and re-exported by `src/components/app/types.ts` for application and settings props.
- `APP_LANGUAGE_NATIVE_LABELS` in the same file is the source for labels displayed by `AppearanceSection`. The `settings.appearance.language.*` entries found in some locale JSON files are not used by that selector and must not be treated as its registry.
- Imports and the `resources` object in `src/i18n/index.ts` register the translation tree that i18next can load for each code.
- `src/i18n/locales/<code>.json` supplies the actual resource. `en.json` defines the canonical keys, value shapes, and interpolation placeholders.

`useAppLanguage` exposes the normalized active language and delegates changes to i18next. Its call to `useTranslation()` subscribes the owning React component to language changes. `AppearanceSection` builds its options from `APP_LANGUAGES` and `APP_LANGUAGE_NATIVE_LABELS`, so a resource alone does not make a language selectable.

## Initial selection, changes, and persistence

`src/i18n/index.ts` initializes i18next when the module is imported. The initial language is resolved in this order:

1. Normalize the value stored at `localStorage["media-tagger.language"]` and use it when supported.
2. Otherwise normalize `window.navigator.language` and use it when supported. `navigator.languages` is not consulted.
3. Otherwise use `en`.

Normalization trims and lowercases the value, then returns the first `APP_LANGUAGES` code for which `normalized.startsWith(code)` is true. This accepts normal region variants such as `fr-FR`, `cs-CZ`, and `zh-CN`, but it is prefix matching rather than full BCP 47 parsing. A non-locale string beginning with a supported code can also match.

On initialization and after every i18next `languageChanged` event, the normalized code is written to both:

- `document.documentElement.lang`, for browser semantics and assistive technology;
- `localStorage["media-tagger.language"]`, for the next launch.

`changeAppLanguage` awaits `i18n.changeLanguage`. `useAppLanguage.setLanguage` intentionally starts that promise without exposing it to UI callers. i18next falls back to English for a key absent from the active resource.

## Translation contract

Treat `src/i18n/locales/en.json` as the canonical tree. For every other resource:

- Every English leaf key must exist at exactly the same object path, with the same value type. Extra or relocated keys are errors, even if the visible wording looks correct.
- Every translated string must contain the same multiset of i18next placeholders as English. Preserve each token exactly, including braces and identifier, for example `{{count}}`, `{{path}}`, or `{{assetId}}`. Translators may move tokens to suit grammar but must not translate, add, remove, or rename them.
- Text around a placeholder may be translated normally. HTML escaping is disabled for interpolation, so do not introduce user-authored markup into translations.
- Do not rely on English fallback to prove completeness. It prevents a missing key from appearing blank, but it also makes partial translations easy to overlook.

### Destructive confirmation words

`lightbox.deleteConfirm.confirmWord` is functional data, not merely copy. `LightboxDeleteConfirmDialog` interpolates it into `typeYesLabel` and `typeYesPlaceholder`, and enables deletion only when the trimmed input compares equal with `localeCompare(..., { sensitivity: "base" })`. Each language therefore needs a deliberate, tested confirmation word as well as the `{{value}}` placeholder in both prompt strings. `common.yes` is unrelated to this gate.

Changing these words changes what a user must type. Verify the displayed prompt and the accepted input in the corresponding language; do not assume machine-generated wording is safe for a destructive control.

## Adding translations

### Add or change a key

1. Add or change the leaf in `src/i18n/locales/en.json` first. Choose the final object path and placeholder identifiers there.
2. Make the same structural edit in all ten non-English resource files. Translate prose, but keep the key path, leaf type, and placeholder multiset identical to English.
3. If the string is a destructive confirmation word or prompt, review the confirmation behavior described above rather than translating it mechanically.
4. Run the read-only parity check below, the focused component tests, and a build. Manually inspect the affected UI in at least English and the changed locale.

### Add a language

1. Add the code at the intended selector position in `APP_LANGUAGES`; the `AppLanguage` type is derived from this tuple.
2. Add its native-language label to `APP_LANGUAGE_NATIVE_LABELS` in `src/i18n/languages.ts`.
3. Create `src/i18n/locales/<code>.json` by copying the complete English tree, then translate every leaf while preserving placeholders and types.
4. Import the JSON and add `{ translation: <import> }` under the same code in the `resources` object in `src/i18n/index.ts`.
5. Choose and test `lightbox.deleteConfirm.confirmWord`, `typeYesLabel`, and `typeYesPlaceholder` explicitly.
6. Extend `AppearanceSection.test.tsx` to select the new value. Add runtime coverage for normalization/persistence if the new code introduces a region or script concern.
7. Run parity, focused tests, a build, and the manual selector/reload checklist. Updating `scripts/generate-locales.mjs` is a separate decision and does not replace any of the preceding registration steps.

## Runtime guarantees

- Only a normalized member of `APP_LANGUAGES` becomes the application language through the normal startup path; unsupported startup values fall back to English.
- English fallback is configured globally.
- The active normalized code is reflected in the document `lang` attribute and persisted under the stable `media-tagger.language` key.
- All 11 locale JSON files are imported into the current runtime resource object.
- The language selector uses stable native labels rather than labels that change with the current UI language.

The offline locale gate checks the corresponding repository invariants. It reads the language tuple without evaluating TypeScript and requires exactly one JSON resource per `AppLanguage`.

## Automated locale contract

English currently has 336 leaf keys. `bun run locale:check` treats `en.json` as canonical and fails when any locale has:

- a missing, extra, or relocated leaf path;
- a different primitive value type; or
- a different multiset of complete i18next placeholder tokens.

The gate also rejects missing or unexpected locale JSON files, duplicate language codes, and a language list that does not start with English. It is read-only and needs no network access. `scripts/locale-tools.test.mjs` covers these failure modes and the generator's preservation policy.

Runtime tests for initial language resolution and persistence remain separate from resource parity. `src/test/setup.ts` imports the real i18n module and requests English for tests. The existing `AppearanceSection` test checks selector callbacks, and the lightbox confirmation tests exercise the English `Yes` flow.

## Locale generator

`bun run locale:generate` reads `en.json` directly and processes every non-English `AppLanguage`, including Polish and Czech. It never parses or executes `src/i18n/index.ts`. By default it translates only canonical leaves that are absent from a locale and preserves every existing value. A complete locale therefore causes no network calls and no file write.

Pass `--overwrite` only for an intentional full machine-translation draft:

```powershell
bun run locale:generate -- --overwrite
```

Both modes preserve the reviewed destructive confirmation words, including `Sì` and `Sí`, and run the offline locale contract after generation. Missing strings still use the unofficial Google Translate endpoint with retries and an in-memory cache. Network output is only a draft. Review wording, accessibility labels, interpolation, layout, and destructive prompts before committing it.

The generator protects placeholders and newlines with sentinel values. The post-generation contract detects a changed or missing token, but it cannot judge translation quality. Do not use `--overwrite` as a routine update command.

## Safe validation

### Read-only parity check

```powershell
bun run locale:check
bun run test:locale-tools
```

Run the relevant existing tests and TypeScript/Vite build after localization changes:

```powershell
bun run test -- src/components/settings/sections/__tests__/AppearanceSection.test.tsx src/components/lightbox/__tests__/LightboxDeleteConfirmDialog.test.tsx
bun run build
```

### Manual checklist

1. Start from a known value by changing only `localStorage["media-tagger.language"]`; do not clear unrelated application storage. Reload and confirm that a supported stored language wins over the browser locale.
2. Remove only that key, reload, and confirm a supported browser locale or region variant is normalized. Confirm an unsupported locale falls back to English.
3. Select every language in Settings. Confirm the UI changes immediately, the selector retains the code, `document.documentElement.lang` equals that code, and the storage key contains that code.
4. Reload after selecting each changed language and confirm persistence.
5. Inspect the changed surfaces for untranslated English fallback, raw translation keys, broken interpolation, clipped text, and incorrect accessible labels.
6. Open the lightbox delete dialog in every affected language. Confirm the prompt displays the reviewed word, wrong input remains blocked, and the localized word enables confirmation.
7. Review the diff before committing. Locale work should not contain generator-driven rewrites outside the intended keys.
