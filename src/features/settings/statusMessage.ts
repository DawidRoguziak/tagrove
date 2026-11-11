import type { useLanguage } from "../../i18n/solid";

export type StatusTranslate = ReturnType<typeof useLanguage>["t"];

/**
 * A section status line. Strings are literal; functions translate through the reactive
 * translator each time the line is read, so a stored message follows the current language.
 */
export type StatusMessage = string | ((t: StatusTranslate) => string);

export function resolveStatusMessage(message: StatusMessage, t: StatusTranslate): string {
  return typeof message === "string" ? message : message(t);
}
