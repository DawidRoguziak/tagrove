import { UiSlider } from "../UI/UiSlider";
import { UiIcon } from "../UI/UiIcon";
import { useTranslation } from "react-i18next";

interface TileSizeSliderProps {
  tileSize: number;
  min: number;
  max: number;
  step: number;
  onChange: (size: number) => void;
  selectionModeEnabled: boolean;
  onToggleSelectionMode: () => void;
}

export function TileSizeSlider({
  tileSize,
  min,
  max,
  step,
  onChange,
  selectionModeEnabled,
  onToggleSelectionMode
}: TileSizeSliderProps) {
  const { t } = useTranslation();
  const bulkActionsLabel = t("controls.bulkActions");
  const bulkActionsToggleLabel = selectionModeEnabled
    ? t("controls.disableBulkActions")
    : t("controls.enableBulkActions");

  return (
    <div className="fixed bottom-3 left-3 z-40 flex max-w-[calc(100vw-1.5rem)] items-center gap-2 rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-[var(--surface-raised)] px-3 py-2 shadow-[var(--shadow-floating)] backdrop-blur-xl">
      <div className="w-[min(162px,calc(100vw-12.5rem))] lg:w-[172px]">
        <UiSlider
          id="tile-size"
          aria-label={t("controls.tileSize")}
          min={min}
          max={max}
          step={step}
          value={tileSize}
          onChange={(event) => onChange(Number(event.target.value))}
        />
      </div>
      <button
        type="button"
        className={`inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg px-2 text-xs font-semibold transition-colors ${
          selectionModeEnabled
            ? "text-primary hover:bg-primary/10"
            : "text-base-content/72 hover:bg-base-200/70 hover:text-base-content"
        }`}
        onClick={onToggleSelectionMode}
        aria-label={bulkActionsToggleLabel}
        aria-pressed={selectionModeEnabled}
        title={bulkActionsToggleLabel}
      >
        <UiIcon name="bulk-actions" className="h-3 w-3 shrink-0" />
        <span className="whitespace-nowrap">{bulkActionsLabel}</span>
      </button>
    </div>
  );
}
