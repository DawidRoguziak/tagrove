import { UiButton } from "../UI/UiButton";
import { UiIcon } from "../UI/UiIcon";

interface GalleryEmptyStateProps {
  hasScanRoots: boolean;
  onAddFirstFolder?: () => void;
  noFoldersTitle: string;
  noFoldersDescription: string;
  addFirstFolderLabel: string;
  noResultsTitle: string;
  noResultsDescription: string;
}

export function GalleryEmptyState({
  hasScanRoots,
  onAddFirstFolder,
  noFoldersTitle,
  noFoldersDescription,
  addFirstFolderLabel,
  noResultsTitle,
  noResultsDescription
}: GalleryEmptyStateProps) {
  return (
    <div
      className="mx-auto grid min-h-full max-w-[540px] place-content-center justify-items-center gap-3 p-7 text-center"
      role="status"
      aria-live="polite"
    >
      <div className="mb-1 grid h-16 w-16 place-items-center rounded-2xl bg-primary/10 text-primary ring-1 ring-primary/15">
        <UiIcon name="info" className="h-8 w-8" />
      </div>
      {!hasScanRoots ? (
        <>
          <h3 className="m-0 text-2xl leading-tight text-base-content">{noFoldersTitle}</h3>
          <p className="m-0 max-w-[420px] text-sm leading-[1.45] text-base-content/65">
            {noFoldersDescription}
          </p>
          <UiButton variant="primary" onClick={onAddFirstFolder}>{addFirstFolderLabel}</UiButton>
        </>
      ) : (
        <>
          <h3 className="m-0 text-2xl leading-tight text-base-content">{noResultsTitle}</h3>
          <p className="m-0 max-w-[420px] text-sm leading-[1.45] text-base-content/65">
            {noResultsDescription}
          </p>
        </>
      )}
    </div>
  );
}
