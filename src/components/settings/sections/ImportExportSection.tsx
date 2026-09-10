import { UiButton } from "../../UI/UiButton";
import { SectionOperationStatus } from "../SectionOperationStatus";
import type { SectionOperationState } from "../types";
import { useTranslation } from "react-i18next";

interface ImportExportSectionProps {
  isOperationLocked: boolean;
  operationState: SectionOperationState;
  onExportCsv: () => void;
  onImportCsv: () => void;
  onExportDbBundle: () => void;
  onImportDbBundle: () => void;
}

export function ImportExportSection({
  isOperationLocked,
  operationState,
  onExportCsv,
  onImportCsv,
  onExportDbBundle,
  onImportDbBundle
}: ImportExportSectionProps) {
  const { t } = useTranslation();

  return (
    <section className="grid min-h-full gap-4 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-surface)]">
      <h3 className="m-0 text-base">{t("settings.importExport.heading")}</h3>
      <div className="settings-operation">
        <p>{t("settings.importExport.csvDescription")}</p>
        <div className="settings-operation-actions">
          <UiButton onClick={onExportCsv} disabled={isOperationLocked}>
            {t("settings.importExport.exportCsv")}
          </UiButton>
          <UiButton onClick={onImportCsv} disabled={isOperationLocked}>
            {t("settings.importExport.importCsv")}
          </UiButton>
        </div>
      </div>
      <div className="settings-operation">
        <p>{t("settings.importExport.bundleDescription")}</p>
        <div className="settings-operation-actions">
          <UiButton onClick={onExportDbBundle} disabled={isOperationLocked}>
            {t("settings.importExport.exportDbBundle")}
          </UiButton>
          <UiButton onClick={onImportDbBundle} disabled={isOperationLocked}>
            {t("settings.importExport.importDbBundle")}
          </UiButton>
        </div>
      </div>

      <SectionOperationStatus state={operationState} loaderTestId="import-export-section-loader" />

      <p className="m-0 text-xs leading-[1.4] text-error">
        {t("settings.importExport.hint")}
      </p>
    </section>
  );
}
