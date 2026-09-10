import { UiSlider } from "../UI/UiSlider";
import { UiButton } from "../UI/UiButton";
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
    <div className="workspace-sizing">
      <UiButton variant={selectionModeEnabled ? "primary" : "ghost"} className="h-8! min-h-8! gap-2 text-xs"
        onClick={onToggleSelectionMode} aria-label={bulkActionsToggleLabel}
        aria-pressed={selectionModeEnabled} title={bulkActionsToggleLabel}>
        <UiIcon name="bulk-actions" className="h-4 w-4" />
        {bulkActionsLabel}
      </UiButton>
      <div className="flex items-center gap-3 border-l border-[var(--border-soft)] pl-4">
        <label htmlFor="tile-size" className="text-xs text-[var(--text-muted)]">{t("controls.tileSize")}</label>
        <div className="flex w-28 items-center py-3">
          <UiSlider id="tile-size" aria-label={t("controls.tileSize")} min={min} max={max} step={step}
            value={tileSize} onChange={(event) => onChange(Number(event.target.value))} />
        </div>
      </div>
    </div>
  );
}
