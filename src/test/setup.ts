import "./setup-dom";
import { cleanup } from "@solidjs/testing-library";
import { afterEach } from "vitest";

afterEach(cleanup);

const { default: i18n, i18nReady } = await import("../i18n/core");
await i18nReady;
await i18n.changeLanguage("en");
