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

- `AppLanguage` in `src/components/app/types.ts` is the compile-time union used by application and settings props.
- `APP_LANGUAGES` in `src/i18n/languages.ts` is the ordered runtime allowlist. It controls selector order and language normalization.
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

1. Add its code to the `AppLanguage` union in `src/components/app/types.ts`.
2. Add the code at the intended selector position in `APP_LANGUAGES` and add its native-language label to `APP_LANGUAGE_NATIVE_LABELS` in `src/i18n/languages.ts`.
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

These are runtime behaviors, not a guarantee that every translation file is complete. Key and placeholder parity is a repository invariant that currently depends on review and validation.

## Current limitations and parity gaps

As of the current repository state, English has 316 leaf keys. A read-only comparison found:

- `fr.json` has 233 leaves: 83 canonical keys are missing and there are no extra keys. The missing keys are all four `settings.status.ready` leaves; 15 direct `settings.progress` leaves; all five `settings.progress.thumbnailSummary` leaves; 13 `settings.actions.pending` leaves; 14 `settings.actions.errorPrefix` leaves; 25 `settings.actions.summary` leaves; and seven `settings.actions.dialogs` leaves.
- `cs.json` has 237 leaves: 85 canonical keys are missing and six extra keys are misplaced. Its missing settings groups are the same as French except that its four `settings.status.ready` leaves are present. In addition, the six canonical `validation.*` leaves are missing because they currently exist under `settings.validation.*`: `fileNameCannotBeEmpty`, `fileNameInvalid`, `fileNameInvalidCharacters`, `duplicatePayloadInvalid`, `unknownAssetInChanges`, and `duplicateGroupStillUnresolved`.
- For keys shared with English, French and Czech currently have no placeholder mismatches. The other eight non-English resources (`pl`, `de`, `it`, `es`, `ru`, `zh`, `ja`, and `ko`) match all 316 English leaf paths and placeholder multisets.

The French and Czech missing keys render through English fallback. This is a graceful runtime result, but it is not translation parity.

There are no dedicated automated tests for initial language resolution, prefix normalization, local-storage persistence, document `lang`, or whole-resource key/placeholder parity. `src/test/setup.ts` imports the real i18n module and requests English for tests. The existing `AppearanceSection` test checks the English selector and callbacks for Polish and Czech; the lightbox confirmation tests exercise the English `Yes` flow.

## Locale generator: behavior and limitations

`scripts/generate-locales.mjs` is a bulk overwrite tool, not a validation or incremental-update tool. Its intended behavior is to:

- read the English and Polish trees from the `resources` declaration in `src/i18n/index.ts`;
- rewrite `en.json` and `pl.json` from those trees;
- collect unique English leaf strings, translate each unique string once, and recursively rebuild the same tree for French, German, Italian, Spanish, Russian, Simplified Chinese, Japanese, and Korean;
- protect placeholders matching `{{ <word-or-dot> }}` and newlines with temporary sentinel strings during translation;
- call the unofficial, unauthenticated `https://translate.googleapis.com/translate_a/single` endpoint with `client=gtx` and source language `en`, using network access, an in-memory cache, a 40 ms delay after successes, and up to five attempts with increasing delays;
- overwrite each target JSON file wholesale, then force a language-specific `lightbox.deleteConfirm.confirmWord`.

Do not run the generator in its current state:

- Its parser searches for a `resources` object ending in `} as const;`, but the current `src/i18n/index.ts` object ends in `};`. It therefore fails before translation with “Cannot find resources object boundaries”.
- Even if that boundary search were changed alone, the current object contains imported identifiers such as `en` and `pl`. The generator evaluates the object literal with `Function(...)`, where those imports are not defined, so the current imported-JSON structure remains incompatible.
- Czech is absent from `TARGET_LANGUAGES`, so it would not be generated even after the parser was repaired. Polish is only rewritten from the extracted tree, not translated.
- The generator hard-codes unaccented `Si` for Italian and Spanish, while the checked-in resources currently use `Sì` and `Sí`. A successful run would overwrite those reviewed confirmation words.
- It overwrites reviewed translations, has no on-disk translation cache, depends on an unofficial endpoint and response shape, and performs no post-generation key, placeholder, fluency, accessibility, or destructive-action validation. Sentinel strings could also be changed by the translation service; restoration only replaces exact sentinels.

Use manual edits and review for current localization work. Repairing or replacing the generator should be a separate change with explicit review of its network and overwrite behavior.

## Safe validation

### Read-only parity check

The following command reads JSON files only. It compares leaf paths and placeholder multisets against English and exits nonzero when it finds a mismatch. The known French and Czech gaps above mean it currently reports failures.

```powershell
node -e '
const fs = require("node:fs");
const dir = "src/i18n/locales";
const load = (code) => JSON.parse(fs.readFileSync(`${dir}/${code}.json`, "utf8"));
const flatten = (value, path = "", out = new Map()) => {
  if (Array.isArray(value)) value.forEach((item, index) => flatten(item, `${path}[${index}]`, out));
  else if (value && typeof value === "object") {
    for (const [key, item] of Object.entries(value)) flatten(item, path ? `${path}.${key}` : key, out);
  } else out.set(path, value);
  return out;
};
const tokens = (value) => typeof value === "string"
  ? [...value.matchAll(/\{\{\s*([\w.]+)\s*\}\}/g)].map((match) => match[1]).sort()
  : [];
const english = flatten(load("en"));
for (const code of ["pl", "fr", "de", "it", "es", "ru", "zh", "ja", "ko", "cs"]) {
  const locale = flatten(load(code));
  const missing = [...english.keys()].filter((key) => !locale.has(key));
  const extra = [...locale.keys()].filter((key) => !english.has(key));
  const placeholders = [...english.keys()].filter((key) => locale.has(key)
    && JSON.stringify(tokens(english.get(key))) !== JSON.stringify(tokens(locale.get(key))));
  console.log(code, { missing, extra, placeholders });
  if (missing.length || extra.length || placeholders.length) process.exitCode = 1;
}
'
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
