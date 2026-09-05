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
      className="mt-4 border-t border-[var(--border-soft)] px-5 py-3 text-xs text-primary"
      role="status"
      aria-live="polite"
    >
      {generatingLabel}
    </div>
  );
  }

  if (!isGeneratingThumbnails && !hasMore) {
    return (
      <div className="mt-4 border-t border-[var(--border-soft)] px-5 py-3 text-xs text-base-content/65">
        {noMoreLabel}
      </div>
    );
  }

  return null;
}
