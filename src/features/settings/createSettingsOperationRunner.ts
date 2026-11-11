import { createSignal, onCleanup } from "solid-js";
import { listen } from "../../platform/tauri/events";
import { useLanguage } from "../../i18n/solid";
import { translateBackendText } from "../../i18n/backendText";
import { createSectionOperationState } from "./operationStateService";
import { translateProgressMessage } from "./progressService";
import type { ScanProgress } from "../../platform/tauri/contracts/scanning";
import type {
  SectionOperationState,
  SettingsOperationSection
} from "./types";

interface OperationOptions {
  section: SettingsOperationSection;
  pendingMessage: string;
  errorPrefix: string;
  progressPhaseMatcher?: (phase: string) => boolean;
  setGlobalLoading?: boolean;
}

export function createSettingsOperationRunner({
  setLoading
}: {
  setLoading: (value: boolean) => void;
}) {
  const { t } = useLanguage();
  const [locked, setLocked] = createSignal(false);
  const sections = {
    scan: createSignal(createSectionOperationState("scan")),
    importExport: createSignal(createSectionOperationState("importExport")),
    danger: createSignal(createSectionOperationState("danger")),
    duplicates: createSignal(createSectionOperationState("duplicates"))
  };
  let operationLock = false;
  let disposed = false;
  let unlisten: (() => void) | undefined;
  onCleanup(() => {
    disposed = true;
    unlisten?.();
    unlisten = undefined;
  });

  function update(
    section: SettingsOperationSection,
    updater: (state: SectionOperationState) => SectionOperationState
  ) {
    if (!disposed) sections[section][1](updater);
  }
  function setSectionMessage(
    section: SettingsOperationSection,
    message: string,
    progress: ScanProgress | null = null
  ) {
    update(section, (state) => ({ ...state, message, progress }));
  }
  async function runExclusiveOperation(options: OperationOptions, action: () => Promise<void>) {
    if (operationLock || disposed) return;
    operationLock = true;
    setLocked(true);
    const globalLoading = options.setGlobalLoading ?? true;
    if (globalLoading) setLoading(true);
    update(options.section, (state) => ({
      ...state,
      loading: true,
      message: options.pendingMessage,
      progress: null
    }));
    let active = true;
    try {
      if (options.progressPhaseMatcher) {
        const stop = await listen<ScanProgress>("process-progress", (event) => {
          if (!active || disposed || !options.progressPhaseMatcher?.(event.payload.phase)) return;
          update(options.section, (state) => ({
            ...state,
            message: translateProgressMessage(event.payload),
            progress: event.payload
          }));
        });
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
      }
      if (!disposed) await action();
    } catch (error) {
      setSectionMessage(
        options.section,
        `${options.errorPrefix}: ${translateBackendText(String(error), t)}`
      );
    } finally {
      active = false;
      try {
        unlisten?.();
      } finally {
        unlisten = undefined;
        update(options.section, (state) => ({ ...state, loading: false }));
        if (globalLoading) setLoading(false);
        operationLock = false;
        setLocked(false);
      }
    }
  }
  return {
    // Admission reads the synchronous lock even before Solid publishes the signal.
    get isOperationLocked() {
      locked();
      return operationLock;
    },
    get scanOperationState() {
      return sections.scan[0]();
    },
    get importExportOperationState() {
      return sections.importExport[0]();
    },
    get dangerOperationState() {
      return sections.danger[0]();
    },
    get duplicateOperationState() {
      return sections.duplicates[0]();
    },
    setSectionMessage,
    runExclusiveOperation
  };
}
