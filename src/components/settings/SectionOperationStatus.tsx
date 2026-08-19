import { UiProgressBar } from "../UI/UiProgressBar";
import { formatProgressLabel } from "./services/progressService";
import type { SectionOperationState } from "./types";

interface SectionOperationStatusProps {
  state: SectionOperationState;
  loaderTestId: string;
}

export function SectionOperationStatus({ state, loaderTestId }: SectionOperationStatusProps) {
  return (
    <div className="grid gap-1">
      {state.loading ? (
        <div
          className="inline-flex items-center gap-2 text-xs text-base-content/65"
          role="status"
          data-testid={loaderTestId}
        >
          <span className="loading loading-spinner loading-xs" aria-hidden="true" />
          <span>{state.message}</span>
        </div>
      ) : (
        <div className="text-xs text-base-content/65">{state.message}</div>
      )}

      {state.progress && (
        <UiProgressBar
          value={state.progress.total > 0 ? state.progress.processed : undefined}
          max={state.progress.total || 1}
          label={formatProgressLabel(state.progress)}
        />
      )}
    </div>
  );
}
