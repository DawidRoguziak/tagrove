import { UiButton } from "../../../ui/controls/UiButton";
import { SectionOperationStatus } from "../SectionOperationStatus";
import type { SectionOperationState } from "../types";
import { useLanguage } from "../../../i18n/solid";
interface DuplicatesSectionProps {
  isOperationLocked: boolean;
  operationState: SectionOperationState;
  duplicateGroups: number;
  duplicateAssets: number;
  onStart: () => void;
}
export function DuplicatesSection(props: DuplicatesSectionProps) {
  const { t } = useLanguage();
  return (
    <section class="grid min-h-full content-start gap-4 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-surface)]">
      <h3 class="m-0 text-lg">{t("settings.duplicates.heading")}</h3>
      <div class="settings-operation">
        <p class="m-0 text-sm leading-relaxed text-[var(--text-muted)]">
          {t("settings.duplicates.description")}
        </p>

        <div class="flex flex-wrap items-center gap-2">
          <UiButton variant="primary" onClick={props.onStart} disabled={props.isOperationLocked}>
            {t("settings.duplicates.start")}
          </UiButton>
          {props.duplicateGroups > 0 ? (
            <span class="text-xs text-[var(--text-muted)]">
              {t("settings.duplicates.lastResult", {
                groups: props.duplicateGroups,
                assets: props.duplicateAssets
              })}
            </span>
          ) : null}
        </div>
      </div>
      <SectionOperationStatus
        state={props.operationState}
        loaderTestId="duplicates-section-loader"
      />
    </section>
  );
}
