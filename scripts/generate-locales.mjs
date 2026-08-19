import { mkdir, readFile, writeFile } from "node:fs/promises";

const I18N_INDEX_PATH = "src/i18n/index.ts";
const LOCALES_DIR = "src/i18n/locales";

const TARGET_LANGUAGES = [
  { code: "fr", api: "fr", confirmWord: "Oui" },
  { code: "de", api: "de", confirmWord: "Ja" },
  { code: "it", api: "it", confirmWord: "Si" },
  { code: "es", api: "es", confirmWord: "Si" },
  { code: "ru", api: "ru", confirmWord: "Да" },
  { code: "zh", api: "zh-CN", confirmWord: "是" },
  { code: "ja", api: "ja", confirmWord: "はい" },
  { code: "ko", api: "ko", confirmWord: "예" }
];

const PLACEHOLDER_REGEX = /\{\{\s*[\w.]+\s*\}\}/g;

function extractResources(indexContent) {
  const marker = "const resources =";
  const start = indexContent.indexOf(marker);
  if (start < 0) {
    throw new Error("Cannot find resources declaration in src/i18n/index.ts");
  }

  const objectStart = indexContent.indexOf("{", start);
  const objectEndMarker = "} as const;";
  const objectEnd = indexContent.indexOf(objectEndMarker, objectStart);
  if (objectStart < 0 || objectEnd < 0) {
    throw new Error("Cannot find resources object boundaries in src/i18n/index.ts");
  }

  const objectLiteral = indexContent.slice(objectStart, objectEnd + 1);
  return Function(`return (${objectLiteral});`)();
}

function collectUniqueStrings(node, bucket) {
  if (typeof node === "string") {
    bucket.add(node);
    return;
  }

  if (Array.isArray(node)) {
    for (const entry of node) {
      collectUniqueStrings(entry, bucket);
    }
    return;
  }

  if (node && typeof node === "object") {
    for (const value of Object.values(node)) {
      collectUniqueStrings(value, bucket);
    }
  }
}

function mapTranslationTree(node, lookup) {
  if (typeof node === "string") {
    return lookup.get(node) ?? node;
  }

  if (Array.isArray(node)) {
    return node.map((entry) => mapTranslationTree(entry, lookup));
  }

  if (node && typeof node === "object") {
    const mapped = {};
    for (const [key, value] of Object.entries(node)) {
      mapped[key] = mapTranslationTree(value, lookup);
    }
    return mapped;
  }

  return node;
}

function protectTemplateTokens(text) {
  const tokens = [];
  let protectedText = text.replace(PLACEHOLDER_REGEX, (match) => {
    const token = `__TPL_${tokens.length}__`;
    tokens.push({ token, value: match });
    return token;
  });

  protectedText = protectedText.replace(/\n/g, "__NL__");
  return { protectedText, tokens };
}

function restoreTemplateTokens(text, tokens) {
  let restored = text;
  for (const token of tokens) {
    restored = restored.split(token.token).join(token.value);
  }
  return restored.split("__NL__").join("\n");
}

function parseGoogleTranslateResponse(payload) {
  if (!Array.isArray(payload) || !Array.isArray(payload[0])) {
    throw new Error("Unexpected translation response format");
  }

  return payload[0]
    .map((chunk) => (Array.isArray(chunk) && typeof chunk[0] === "string" ? chunk[0] : ""))
    .join("");
}

async function sleep(ms) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function translateText(text, targetApiLanguage, cache) {
  const cacheKey = `${targetApiLanguage}\u0000${text}`;
  if (cache.has(cacheKey)) {
    return cache.get(cacheKey);
  }

  const { protectedText, tokens } = protectTemplateTokens(text);
  const url =
    "https://translate.googleapis.com/translate_a/single?client=gtx&sl=en&dt=t" +
    `&tl=${encodeURIComponent(targetApiLanguage)}` +
    `&q=${encodeURIComponent(protectedText)}`;

  let lastError = null;
  for (let attempt = 0; attempt < 5; attempt += 1) {
    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Translation HTTP error ${response.status}`);
      }

      const payload = await response.json();
      const translated = parseGoogleTranslateResponse(payload);
      const restored = restoreTemplateTokens(translated, tokens);
      cache.set(cacheKey, restored);
      await sleep(40);
      return restored;
    } catch (error) {
      lastError = error;
      await sleep(300 * (attempt + 1));
    }
  }

  throw new Error(`Failed to translate text after retries: ${text}\n${String(lastError)}`);
}

async function main() {
  const indexContent = await readFile(I18N_INDEX_PATH, "utf8");
  const resources = extractResources(indexContent);

  const enTranslation = resources?.en?.translation;
  const plTranslation = resources?.pl?.translation;
  if (!enTranslation || !plTranslation) {
    throw new Error("Missing en/pl translations in resources object");
  }

  await mkdir(LOCALES_DIR, { recursive: true });
  await writeFile(`${LOCALES_DIR}/en.json`, `${JSON.stringify(enTranslation, null, 2)}\n`, "utf8");
  await writeFile(`${LOCALES_DIR}/pl.json`, `${JSON.stringify(plTranslation, null, 2)}\n`, "utf8");

  const uniqueEnglishStrings = new Set();
  collectUniqueStrings(enTranslation, uniqueEnglishStrings);
  const sourceStrings = [...uniqueEnglishStrings];
  const cache = new Map();

  console.log(`Source unique strings: ${sourceStrings.length}`);

  for (const language of TARGET_LANGUAGES) {
    console.log(`Translating ${language.code}...`);
    const lookup = new Map();

    let index = 0;
    for (const sourceText of sourceStrings) {
      const translated = await translateText(sourceText, language.api, cache);
      lookup.set(sourceText, translated);
      index += 1;
      if (index % 50 === 0 || index === sourceStrings.length) {
        console.log(`${language.code}: ${index}/${sourceStrings.length}`);
      }
    }

    const translatedTree = mapTranslationTree(enTranslation, lookup);
    translatedTree.lightbox.deleteConfirm.confirmWord = language.confirmWord;

    await writeFile(
      `${LOCALES_DIR}/${language.code}.json`,
      `${JSON.stringify(translatedTree, null, 2)}\n`,
      "utf8"
    );
  }

  console.log("Locale generation complete.");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
