import { UiSlider } from "../../ui/controls/UiSlider";
import { UiButton } from "../../ui/controls/UiButton";
import { UiIcon } from "../../ui/controls/UiIcon";
import { useLanguage } from "../../i18n/solid";
interface TileSizeSliderProps {
  tileSize: number;
  min: number;
  max: number;
  step: number;
  onChange: (size: number) => void;
  selectionModeEnabled: boolean;
  onToggleSelectionMode: () => void;
}
export function TileSizeSlider(props: TileSizeSliderProps) {
  const { t } = useLanguage();
  const bulkActionsLabel = () => t("controls.bulkActions");
  const bulkActionsToggleLabel = () =>
    props.selectionModeEnabled ? t("controls.disableBulkActions") : t("controls.enableBulkActions");
  return (
    <div class="workspace-sizing">
      <UiButton
        variant={props.selectionModeEnabled ? "primary" : "ghost"}
        className="h-8! min-h-8! gap-2 text-xs"
        onClick={props.onToggleSelectionMode}
        aria-label={bulkActionsToggleLabel()}
        aria-pressed={props.selectionModeEnabled ? "true" : "false"}
        title={bulkActionsToggleLabel()}
      >
        <UiIcon name="bulk-actions" className="h-4 w-4" />
        {bulkActionsLabel()}
      </UiButton>
      <div class="flex items-center gap-3 border-l border-[var(--border-soft)] pl-4">
        <label for="tile-size" class="text-xs text-[var(--text-muted)]">
          {t("controls.tileSize")}
        </label>
        <div class="flex w-28 items-center py-3">
          <UiSlider
            id="tile-size"
            aria-label={t("controls.tileSize")}
            min={props.min}
            max={props.max}
            step={props.step}
            value={props.tileSize}
            onInput={(event) => props.onChange(Number(event.currentTarget.value))}
          />
        </div>
      </div>
    </div>
  );
}
