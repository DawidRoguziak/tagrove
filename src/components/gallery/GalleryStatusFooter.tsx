interface GalleryStatusFooterProps {
  isGeneratingThumbnails: boolean;
  hasMore: boolean;
  generatingLabel: string;
  noMoreLabel: string;
}

export function GalleryStatusFooter({
  isGeneratingThumbnails,
  hasMore,
  generatingLabel,
  noMoreLabel
}: GalleryStatusFooterProps) {
  if (isGeneratingThumbnails && hasMore) {
    return (
      <div
      className="mx-auto mb-1 mt-3.5 w-fit rounded-full bg-primary px-3.5 py-2 text-xs text-primary-content shadow-[var(--shadow-floating)]"
      role="status"
      aria-live="polite"
    >
      {generatingLabel}
    </div>
  );
  }

  if (!isGeneratingThumbnails && !hasMore) {
    return (
      <div className="mx-auto mb-1 mt-3.5 w-fit rounded-full bg-base-300/85 px-3.5 py-2 text-xs text-base-content shadow-[var(--shadow-surface)]">
        {noMoreLabel}
      </div>
    );
  }

  return null;
}
