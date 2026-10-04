import { UiButton } from "../../../ui/controls/UiButton";
import { SectionOperationStatus } from "../SectionOperationStatus";
import type { SectionOperationState } from "../../../components/settings/types";
import { useLanguage } from "../../../i18n/solid";
interface DangerZoneSectionProps {
  isOperationLocked: boolean;
  operationState: SectionOperationState;
  onOpenClearConfirm: () => void;
}
export function DangerZoneSection(props: DangerZoneSectionProps) {
  const { t } = useLanguage();
  return (
    <section class="grid gap-4 rounded-[var(--radius-surface)] border border-error/20 bg-error/5 p-5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center ">
      <div>
        <h3 class="m-0 text-base text-error">{t("settings.danger.heading")}</h3>
        <p class="m-0 mt-1 max-w-[760px] text-xs leading-relaxed text-[var(--text-muted)]">
          {t("settings.danger.hint")}
        </p>
      </div>
      <UiButton
        variant="danger"
        onClick={props.onOpenClearConfirm}
        disabled={props.isOperationLocked}
      >
        {t("settings.danger.clearLibrary")}
      </UiButton>

      <SectionOperationStatus state={props.operationState} loaderTestId="danger-section-loader" />
    </section>
  );
}
