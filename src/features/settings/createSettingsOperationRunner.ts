import { createSignal, onCleanup } from "solid-js";
import { listen } from "../../platform/tauri/events";
import { useLanguage } from "../../i18n/solid";
import { translateBackendText } from "../../i18n/backendText";
import { createSectionStatusState } from "./operationStateService";
import { translateProgressMessage } from "./progressService";
import { resolveStatusMessage, type StatusMessage } from "./statusMessage";
import type { ScanProgress } from "../../platform/tauri/contracts/scanning";
import type {
  SectionOperationState,
  SectionStatusState,
  SettingsOperationSection
} from "./types";

interface OperationOptions {
  section: SettingsOperationSection;
  pendingMessage: StatusMessage;
  errorPrefix: StatusMessage;
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
    scan: createSignal(createSectionStatusState("scan")),
    importExport: createSignal(createSectionStatusState("importExport")),
    danger: createSignal(createSectionStatusState("danger")),
    duplicates: createSignal(createSectionStatusState("duplicates"))
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
    updater: (state: SectionStatusState) => SectionStatusState
  ) {
    if (!disposed) sections[section][1](updater);
  }
  function setSectionMessage(
    section: SettingsOperationSection,
    message: StatusMessage,
    progress: ScanProgress | null = null
  ) {
    update(section, (state) => ({ ...state, message, progress }));
  }
  // The stored message is translated when it is read, which renders track the language.
  function view(state: SectionStatusState): SectionOperationState {
    return {
      loading: state.loading,
      progress: state.progress,
      get message() {
        return resolveStatusMessage(state.message, t);
      }
    };
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
            message: (translate) => translateProgressMessage(event.payload, translate),
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
      const detail = String(error);
      setSectionMessage(
        options.section,
        (translate) =>
          `${resolveStatusMessage(options.errorPrefix, translate)}: ${translateBackendText(detail, translate)}`
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
      return view(sections.scan[0]());
    },
    get importExportOperationState() {
      return view(sections.importExport[0]());
    },
    get dangerOperationState() {
      return view(sections.danger[0]());
    },
    get duplicateOperationState() {
      return view(sections.duplicates[0]());
    },
    setSectionMessage,
    runExclusiveOperation
  };
}
