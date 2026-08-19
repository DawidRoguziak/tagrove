import { useCallback, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import {
  createSectionOperationState
} from "../services/operationStateService";
import { translateProgressMessage } from "../services/progressService";
import type { ScanProgress } from "../../../types";
import type { SectionOperationState, SettingsOperationSection } from "../types";

interface RunExclusiveOperationOptions {
  section: SettingsOperationSection;
  pendingMessage: string;
  errorPrefix: string;
  progressPhaseMatcher?: (phase: string) => boolean;
  setGlobalLoading?: boolean;
}

interface UseSettingsOperationRunnerOptions {
  setLoading: (value: boolean) => void;
}

export function useSettingsOperationRunner({ setLoading }: UseSettingsOperationRunnerOptions) {
  const { t } = useTranslation();
  const [isOperationLocked, setIsOperationLocked] = useState(false);
  const [scanOperationState, setScanOperationState] = useState<SectionOperationState>(
    createSectionOperationState("scan")
  );
  const [importExportOperationState, setImportExportOperationState] = useState<SectionOperationState>(
    createSectionOperationState("importExport")
  );
  const [dangerOperationState, setDangerOperationState] = useState<SectionOperationState>(
    createSectionOperationState("danger")
  );
  const [duplicateOperationState, setDuplicateOperationState] = useState<SectionOperationState>(
    createSectionOperationState("duplicates")
  );
  const operationLockRef = useRef(false);

  const translateBackendText = useCallback(
    (raw: string) => {
      const trimmed = raw.trim();
      if (!trimmed) {
        return raw;
      }

      if (trimmed.startsWith("i18n:")) {
        const key = trimmed.slice(5).trim();
        if (key.length > 0) {
          return t(key);
        }
      }

      if (trimmed.startsWith("{")) {
        try {
          const parsed = JSON.parse(trimmed) as { key?: string; params?: Record<string, unknown> };
          if (typeof parsed.key === "string" && parsed.key.trim().length > 0) {
            return t(parsed.key, parsed.params ?? {});
          }
        } catch {
          // ignore invalid payload and keep raw message
        }
      }

      if (/^[a-z0-9_.-]+$/i.test(trimmed) && trimmed.includes(".")) {
        const translated = t(trimmed);
        if (translated !== trimmed) {
          return translated;
        }
      }

      return raw;
    },
    [t]
  );

  const formatOperationError = useCallback(
    (errorPrefix: string, error: unknown) => {
      return `${errorPrefix}: ${translateBackendText(String(error))}`;
    },
    [translateBackendText]
  );

  const updateSectionOperationState = useCallback(
    (
      section: SettingsOperationSection,
      updater: (current: SectionOperationState) => SectionOperationState
    ) => {
      if (section === "scan") {
        setScanOperationState(updater);
        return;
      }
      if (section === "importExport") {
        setImportExportOperationState(updater);
        return;
      }
      if (section === "danger") {
        setDangerOperationState(updater);
        return;
      }
      setDuplicateOperationState(updater);
    },
    []
  );

  const setSectionMessage = useCallback(
    (section: SettingsOperationSection, message: string, progress: ScanProgress | null = null) => {
      updateSectionOperationState(section, (current) => ({
        ...current,
        message,
        progress
      }));
    },
    [updateSectionOperationState]
  );

  const runExclusiveOperation = useCallback(
    async (options: RunExclusiveOperationOptions, action: () => Promise<void>) => {
      if (operationLockRef.current) {
        return;
      }

      operationLockRef.current = true;
      setIsOperationLocked(true);

      const shouldSetGlobalLoading = options.setGlobalLoading ?? true;
      if (shouldSetGlobalLoading) {
        setLoading(true);
      }

      updateSectionOperationState(options.section, (current) => ({
        ...current,
        loading: true,
        message: options.pendingMessage,
        progress: null
      }));

      let unlisten: (() => void) | null = null;

      try {
        if (options.progressPhaseMatcher) {
          unlisten = await listen<ScanProgress>("process-progress", (event) => {
            if (!options.progressPhaseMatcher?.(event.payload.phase)) {
              return;
            }

            updateSectionOperationState(options.section, (current) => ({
              ...current,
              message: translateProgressMessage(event.payload),
              progress: event.payload
            }));
          });
        }

        await action();
      } catch (error) {
        setSectionMessage(options.section, formatOperationError(options.errorPrefix, error), null);
      } finally {
        if (unlisten) {
          unlisten();
        }

        updateSectionOperationState(options.section, (current) => ({
          ...current,
          loading: false
        }));

        if (shouldSetGlobalLoading) {
          setLoading(false);
        }
        operationLockRef.current = false;
        setIsOperationLocked(false);
      }
    },
    [formatOperationError, setLoading, setSectionMessage, updateSectionOperationState]
  );

  return {
    isOperationLocked,
    scanOperationState,
    importExportOperationState,
    dangerOperationState,
    duplicateOperationState,
    setSectionMessage,
    runExclusiveOperation
  };
}
