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
    <section className="grid min-h-full gap-4 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-raised)] p-5 shadow-[var(--shadow-surface)] sm:p-6">
      <h3 className="m-0 text-lg">{t("settings.importExport.heading")}</h3>
      <div className="grid gap-2 sm:grid-cols-2">
        <UiButton onClick={onExportCsv} disabled={isOperationLocked}>
          {t("settings.importExport.exportCsv")}
        </UiButton>
        <UiButton onClick={onImportCsv} disabled={isOperationLocked}>
          {t("settings.importExport.importCsv")}
        </UiButton>
      </div>
      <div className="grid gap-2 sm:grid-cols-2">
        <UiButton onClick={onExportDbBundle} disabled={isOperationLocked}>
          {t("settings.importExport.exportDbBundle")}
        </UiButton>
        <UiButton onClick={onImportDbBundle} disabled={isOperationLocked}>
          {t("settings.importExport.importDbBundle")}
        </UiButton>
      </div>

      <SectionOperationStatus state={operationState} loaderTestId="import-export-section-loader" />

      <p className="m-0 text-xs leading-[1.4] text-error/85">
        {t("settings.importExport.hint")}
      </p>
    </section>
  );
}
