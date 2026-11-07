import { useLanguage } from "../../i18n/solid";
import { UiIcon } from "../../ui/controls/UiIcon";
interface GalleryStatusFooterProps {
  resultCount: number;
}
export function GalleryStatusFooter(props: GalleryStatusFooterProps) {
  const { t, language } = useLanguage();
  return (
    <div
      class="flex min-h-[136px] flex-col items-center justify-center gap-2 px-5 py-4 text-center"
      data-testid="gallery-end-state"
    >
      <div class="flex w-full max-w-sm items-center gap-4" aria-hidden="true">
        <span class="h-px min-w-0 flex-1 bg-[var(--border-soft)]" />
        <span class="flex size-10 shrink-0 items-center justify-center rounded-full bg-[var(--surface-raised)] text-success">
          <UiIcon name="check-circle" className="size-6" />
        </span>
        <span class="h-px min-w-0 flex-1 bg-[var(--border-soft)]" />
      </div>
      <h2 class="max-w-full text-xl font-semibold text-base-content">
        {t("gallery.endOfResults")}
      </h2>
      <p class="flex max-w-full items-center justify-center gap-2 font-mono text-[14px] text-[var(--text-muted)]">
        <span class="size-1.5 shrink-0 rounded-full bg-success" aria-hidden="true" />
        <span class="min-w-0 [overflow-wrap:anywhere]">
          {t("gallery.resultCount", {
            count: props.resultCount,
            formattedCount: new Intl.NumberFormat(language()).format(props.resultCount)
          })}
        </span>
      </p>
    </div>
  );
}
