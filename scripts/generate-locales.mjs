import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { isDeepStrictEqual } from "node:util";
import { fileURLToPath } from "node:url";
import {
  LOCALES_DIR,
  loadAppLanguageCodes,
  readLocale,
  validateAllLocales
} from "./locale-contract.mjs";

const API_LANGUAGES = {
  pl: "pl",
  fr: "fr",
  de: "de",
  it: "it",
  es: "es",
  ru: "ru",
  zh: "zh-CN",
  ja: "ja",
  ko: "ko",
  cs: "cs"
};
const REVIEWED_CONFIRM_WORDS = {
  pl: "Tak",
  fr: "Oui",
  de: "Ja",
  it: "Sì",
  es: "Sí",
  ru: "Да",
  zh: "是",
  ja: "はい",
  ko: "예",
  cs: "Ano"
};
const PLACEHOLDER_REGEX = /\{\{\s*[\w.]+\s*\}\}/g;

export function protectTemplateTokens(text) {
  const tokens = [];
  let protectedText = text.replace(PLACEHOLDER_REGEX, (value) => {
    const token = `__MEDIATAGGER_PLACEHOLDER_${tokens.length}__`;
    tokens.push({ token, value });
    return token;
  });
  protectedText = protectedText.replace(/\n/g, "__MEDIATAGGER_NEWLINE__");
  return { protectedText, tokens };
}

export function restoreTemplateTokens(text, tokens) {
  let restored = text;
  for (const { token, value } of tokens) restored = restored.split(token).join(value);
  return restored.split("__MEDIATAGGER_NEWLINE__").join("\n");
}

function parseGoogleTranslateResponse(payload) {
  if (!Array.isArray(payload) || !Array.isArray(payload[0]))
    throw new Error("Unexpected translation response format");
  return payload[0]
    .map((chunk) => (Array.isArray(chunk) && typeof chunk[0] === "string" ? chunk[0] : ""))
    .join("");
}

async function sleep(milliseconds) {
  await new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function translateText(text, targetLanguage, cache) {
  const cacheKey = `${targetLanguage}\u0000${text}`;
  if (cache.has(cacheKey)) return cache.get(cacheKey);

  const { protectedText, tokens } = protectTemplateTokens(text);
  const url =
    "https://translate.googleapis.com/translate_a/single?client=gtx&sl=en&dt=t" +
    `&tl=${encodeURIComponent(targetLanguage)}&q=${encodeURIComponent(protectedText)}`;
  let lastError;
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    try {
      const response = await fetch(url);
      if (!response.ok) throw new Error(`Translation HTTP error ${response.status}`);
      const translated = restoreTemplateTokens(
        parseGoogleTranslateResponse(await response.json()),
        tokens
      );
      cache.set(cacheKey, translated);
      await sleep(40);
      return translated;
    } catch (error) {
      lastError = error;
      await sleep(300 * attempt);
    }
  }
  throw new Error(`Failed to translate text after retries: ${text}\n${String(lastError)}`);
}

export async function mergeLocaleTree(
  canonical,
  existing,
  translate,
  options = {},
  currentPath = ""
) {
  if (typeof canonical === "string") {
    if (!options.overwrite && typeof existing === "string") return existing;
    return translate(canonical, currentPath);
  }
  if (Array.isArray(canonical)) {
    return Promise.all(
      canonical.map((entry, index) =>
        mergeLocaleTree(entry, existing?.[index], translate, options, `${currentPath}[${index}]`)
      )
    );
  }
  if (canonical && typeof canonical === "object") {
    const output = {};
    for (const [key, value] of Object.entries(canonical)) {
      const childPath = currentPath ? `${currentPath}.${key}` : key;
      output[key] = await mergeLocaleTree(value, existing?.[key], translate, options, childPath);
    }
    return output;
  }
  return options.overwrite || typeof existing !== typeof canonical ? canonical : existing;
}

export function localeChanged(existing, generated) {
  return !isDeepStrictEqual(existing, generated);
}

export async function generateLocales({ overwrite = false, rootDir = process.cwd() } = {}) {
  const codes = await loadAppLanguageCodes(rootDir);
  const canonical = await readLocale("en", rootDir);
  const cache = new Map();

  for (const code of codes.filter((entry) => entry !== "en")) {
    const localePath = path.join(rootDir, LOCALES_DIR, `${code}.json`);
    const existing = JSON.parse(await readFile(localePath, "utf8"));
    let translatedCount = 0;
    const merged = await mergeLocaleTree(
      canonical,
      existing,
      async (text, key) => {
        if (key === "lightbox.deleteConfirm.confirmWord") return REVIEWED_CONFIRM_WORDS[code];
        translatedCount += 1;
        return translateText(text, API_LANGUAGES[code], cache);
      },
      { overwrite }
    );
    merged.lightbox.deleteConfirm.confirmWord = REVIEWED_CONFIRM_WORDS[code];
    if (localeChanged(existing, merged))
      await writeFile(localePath, `${JSON.stringify(merged, null, 2)}\n`, "utf8");
    console.log(`${code}: ${translatedCount} translated value(s)`);
  }

  const { errors } = await validateAllLocales(rootDir);
  if (errors.length > 0)
    throw new Error(`Generated locales failed validation:\n${errors.join("\n")}`);
}

const currentFile = fileURLToPath(import.meta.url);
if (process.argv[1] && path.resolve(process.argv[1]) === currentFile) {
  const unsupported = process.argv.slice(2).filter((argument) => argument !== "--overwrite");
  if (unsupported.length > 0) throw new Error(`Unknown option(s): ${unsupported.join(", ")}`);
  await generateLocales({ overwrite: process.argv.includes("--overwrite") });
}
