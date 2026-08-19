import { UiButton } from "../../UI/UiButton";
import { SectionOperationStatus } from "../SectionOperationStatus";
import type { SectionOperationState } from "../types";
import { useTranslation } from "react-i18next";

interface DangerZoneSectionProps {
  isOperationLocked: boolean;
  operationState: SectionOperationState;
  onOpenClearConfirm: () => void;
}

export function DangerZoneSection({
  isOperationLocked,
  operationState,
  onOpenClearConfirm
}: DangerZoneSectionProps) {
  const { t } = useTranslation();

  return (
    <section className="grid gap-4 rounded-[var(--radius-surface)] border border-error/20 bg-error/5 p-5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center sm:p-6">
      <div>
        <h3 className="m-0 text-lg text-error">{t("settings.danger.heading")}</h3>
        <p className="m-0 mt-1 max-w-[760px] text-xs leading-relaxed text-base-content/58">
          {t("settings.danger.hint")}
        </p>
      </div>
      <UiButton variant="danger" onClick={onOpenClearConfirm} disabled={isOperationLocked}>
        {t("settings.danger.clearLibrary")}
      </UiButton>

      <SectionOperationStatus state={operationState} loaderTestId="danger-section-loader" />
    </section>
  );
}
