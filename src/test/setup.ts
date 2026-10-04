import "./setup-dom";

const { default: i18n } = await import("../i18n");
await i18n.changeLanguage("en");
