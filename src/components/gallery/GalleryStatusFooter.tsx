import { useTranslation } from "react-i18next";
import { UiIcon } from "../UI/UiIcon";

interface GalleryStatusFooterProps {
  isGeneratingThumbnails: boolean;
  hasMore: boolean;
  generatingLabel: string;
  resultCount: number;
}

export function GalleryStatusFooter({
  isGeneratingThumbnails,
  hasMore,
  generatingLabel,
  resultCount
}: GalleryStatusFooterProps) {
  const { t, i18n } = useTranslation();

  if (isGeneratingThumbnails && hasMore) {
    return (
      <div
        className="mt-4 border-t border-[var(--border-soft)] px-5 py-3 text-xs text-primary-text"
        role="status"
        aria-live="polite"
      >
        {generatingLabel}
      </div>
    );
  }

  if (!isGeneratingThumbnails && !hasMore) {
    const formattedCount = new Intl.NumberFormat(i18n.resolvedLanguage ?? i18n.language).format(resultCount);
    return (
      <div className="flex min-h-[136px] flex-col items-center justify-center gap-2 px-5 py-4 text-center" data-testid="gallery-end-state">
        <div className="flex w-full max-w-sm items-center gap-4" aria-hidden="true">
          <span className="h-px min-w-0 flex-1 bg-[var(--border-soft)]" />
          <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-[var(--surface-raised)] text-success">
            <UiIcon name="check-circle" className="size-6" />
          </span>
          <span className="h-px min-w-0 flex-1 bg-[var(--border-soft)]" />
        </div>
        <h2 className="max-w-full text-xl font-semibold text-base-content">{t("gallery.endOfResults")}</h2>
        <p className="flex max-w-full items-center justify-center gap-2 font-mono text-[14px] text-[var(--text-muted)]">
          <span className="size-1.5 shrink-0 rounded-full bg-success" aria-hidden="true" />
          <span className="min-w-0 [overflow-wrap:anywhere]">{t("gallery.resultCount", { count: resultCount, formattedCount })}</span>
        </p>
      </div>
    );
  }

  return null;
}
