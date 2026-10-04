import { UiButton } from "../../ui/controls/UiButton";
import { UiIcon } from "../../ui/controls/UiIcon";
interface GalleryEmptyStateProps {
  hasScanRoots: boolean;
  onAddFirstFolder?: () => void;
  noFoldersTitle: string;
  noFoldersDescription: string;
  addFirstFolderLabel: string;
  noResultsTitle: string;
  noResultsDescription: string;
}
export function GalleryEmptyState(props: GalleryEmptyStateProps) {
  return (
    <div
      class="mx-auto grid min-h-[calc(100dvh-180px)] max-w-[540px] place-content-center justify-items-center gap-3 p-7 text-center"
      role="status"
      aria-live="polite"
    >
      <div class="mb-1 grid h-16 w-16 place-items-center rounded-[var(--radius-control)] bg-primary/10 text-primary-text ring-1 ring-primary/15">
        <UiIcon name={props.hasScanRoots ? "search" : "folder"} className="h-8 w-8" />
      </div>
      {!props.hasScanRoots ? (
        <>
          <h3 class="m-0 text-2xl leading-tight text-base-content">{props.noFoldersTitle}</h3>
          <p class="m-0 max-w-[420px] text-sm leading-[1.45] text-[var(--text-muted)]">
            {props.noFoldersDescription}
          </p>
          <UiButton variant="primary" onClick={props.onAddFirstFolder}>
            {props.addFirstFolderLabel}
          </UiButton>
        </>
      ) : (
        <>
          <h3 class="m-0 text-2xl leading-tight text-base-content">{props.noResultsTitle}</h3>
          <p class="m-0 max-w-[420px] text-sm leading-[1.45] text-[var(--text-muted)]">
            {props.noResultsDescription}
          </p>
        </>
      )}
    </div>
  );
}
