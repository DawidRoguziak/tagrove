import { UiProgressBar } from "../../ui/controls/UiProgressBar";
import { formatProgressLabel } from "../../components/settings/services/progressService";
import type { SectionOperationState } from "../../components/settings/types";
interface SectionOperationStatusProps {
  state: SectionOperationState;
  loaderTestId: string;
}
export function SectionOperationStatus(props: SectionOperationStatusProps) {
  return (
    <div class="grid gap-1">
      {props.state.loading ? (
        <div
          class="inline-flex items-center gap-2 text-xs text-[var(--text-muted)]"
          role="status"
          data-testid={props.loaderTestId}
        >
          <span class="loading loading-spinner loading-xs" aria-hidden="true" />
          <span>{props.state.message}</span>
        </div>
      ) : (
        <div class="text-xs text-[var(--text-muted)]">{props.state.message}</div>
      )}

      {props.state.progress && (
        <UiProgressBar
          value={props.state.progress.total > 0 ? props.state.progress.processed : undefined}
          max={props.state.progress.total || 1}
          label={formatProgressLabel(props.state.progress)}
        />
      )}
    </div>
  );
}
