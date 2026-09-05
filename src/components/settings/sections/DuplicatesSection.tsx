import { UiButton } from "../../UI/UiButton";
import { SectionOperationStatus } from "../SectionOperationStatus";
import type { SectionOperationState } from "../types";
import { useTranslation } from "react-i18next";

interface DuplicatesSectionProps {
  isOperationLocked: boolean;
  operationState: SectionOperationState;
  duplicateGroups: number;
  duplicateAssets: number;
  onStart: () => void;
}

export function DuplicatesSection({
  isOperationLocked,
  operationState,
  duplicateGroups,
  duplicateAssets,
  onStart
}: DuplicatesSectionProps) {
  const { t } = useTranslation();

  return (
    <section className="grid min-h-full content-start gap-4 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-surface)]">
      <h3 className="m-0 text-lg">{t("settings.duplicates.heading")}</h3>
      <p className="m-0 text-sm leading-relaxed text-base-content/60">
        {t("settings.duplicates.description")}
      </p>

      <div className="flex flex-wrap items-center gap-2">
        <UiButton variant="primary" onClick={onStart} disabled={isOperationLocked}>
          {t("settings.duplicates.start")}
        </UiButton>
        {duplicateGroups > 0 ? (
          <span className="text-xs text-base-content/65">
            {t("settings.duplicates.lastResult", {
              groups: duplicateGroups,
              assets: duplicateAssets
            })}
          </span>
        ) : null}
      </div>

      <SectionOperationStatus state={operationState} loaderTestId="duplicates-section-loader" />
    </section>
  );
}
