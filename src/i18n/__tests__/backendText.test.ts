import { describe, expect, it } from "vitest";
import { translateBackendText } from "../backendText";

const messages: Record<string, string> = {
  "common.libraryBusy": "Busy",
  "settings.greeting": "Hello {{name}}"
};
const t = (key: string, options?: Record<string, unknown>) =>
  (messages[key] ?? key).replace("{{name}}", String(options?.name ?? ""));

describe("translateBackendText", () => {
  it("translates i18n keys, JSON keys with params, and known dotted tokens", () => {
    expect(translateBackendText(" i18n:common.libraryBusy ", t)).toBe("Busy");
    expect(
      translateBackendText('{"key":"settings.greeting","params":{"name":"Ada"}}', t)
    ).toBe("Hello Ada");
    expect(translateBackendText("common.libraryBusy", t)).toBe("Busy");
  });
  it("keeps free-form, unknown and malformed text unchanged", () => {
    expect(translateBackendText("disk full", t)).toBe("disk full");
    expect(translateBackendText("unknown.key", t)).toBe("unknown.key");
    expect(translateBackendText("i18n:", t)).toBe("i18n:");
    expect(translateBackendText("{not json", t)).toBe("{not json");
  });
});
