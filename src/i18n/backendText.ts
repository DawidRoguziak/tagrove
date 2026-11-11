type Translate = (key: string, options?: Record<string, unknown>) => string;

/**
 * Translates backend display text: `i18n:<key>`, a `{"key","params"}` JSON string, or a
 * dotted token that resolves to a translation. Anything else is shown unchanged. This is
 * display translation only; callers never branch on the text
 * (docs/subsystems/settings-operations.md, "backend error text").
 */
export function translateBackendText(raw: string, t: Translate): string {
  const trimmed = raw.trim();
  if (trimmed.startsWith("i18n:") && trimmed.slice(5).trim()) return t(trimmed.slice(5).trim());
  if (trimmed.startsWith("{")) {
    try {
      const parsed: unknown = JSON.parse(trimmed);
      if (
        typeof parsed === "object" &&
        parsed !== null &&
        "key" in parsed &&
        typeof parsed.key === "string" &&
        parsed.key.trim()
      ) {
        const params =
          "params" in parsed && typeof parsed.params === "object" && parsed.params !== null
            ? parsed.params
            : {};
        return t(parsed.key, { ...params });
      }
    } catch {
      /* Keep free-form backend errors. */
    }
  }
  if (/^[a-z0-9_.-]+$/i.test(trimmed) && trimmed.includes(".")) {
    const translated = t(trimmed);
    if (translated !== trimmed) return translated;
  }
  return raw;
}
