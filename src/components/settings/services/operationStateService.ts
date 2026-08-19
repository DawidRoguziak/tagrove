import type { SectionOperationState, SettingsOperationSection } from "../types";
import i18n from "../../../i18n";

export const SECTION_READY_MESSAGE_KEYS: Record<SettingsOperationSection, string> = {
  scan: "settings.status.ready.scan",
  importExport: "settings.status.ready.importExport",
  danger: "settings.status.ready.danger",
  duplicates: "settings.status.ready.duplicates"
};

export const SECTION_READY_MESSAGES: Record<SettingsOperationSection, string> = {
  scan: i18n.t(SECTION_READY_MESSAGE_KEYS.scan),
  importExport: i18n.t(SECTION_READY_MESSAGE_KEYS.importExport),
  danger: i18n.t(SECTION_READY_MESSAGE_KEYS.danger),
  duplicates: i18n.t(SECTION_READY_MESSAGE_KEYS.duplicates)
};

export function getSectionReadyMessage(section: SettingsOperationSection): string {
  return i18n.t(SECTION_READY_MESSAGE_KEYS[section]);
}

export function createSectionOperationState(section: SettingsOperationSection): SectionOperationState {
  return {
    loading: false,
    message: getSectionReadyMessage(section),
    progress: null
  };
}
