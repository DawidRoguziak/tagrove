import type { SelectedAsset } from "../../types";
import { formatBytes, formatDuration } from "../../utils/media";
import { useTranslation } from "react-i18next";

interface LightboxInfoPanelProps {
  selected: SelectedAsset;
}

export function LightboxInfoPanel({ selected }: LightboxInfoPanelProps) {
  const { t } = useTranslation();

  return (
    <aside
      aria-label={t("lightbox.infoHeading")}
      data-testid="lightbox-info-panel"
      className="grid gap-1.5 rounded-lg bg-base-200/45 p-2 text-xs text-base-content/82"
    >
      <h3 className="m-0 text-xs">{t("lightbox.infoHeading")}</h3>
      <div className="grid gap-1.5">
        <span><strong className="font-semibold text-base-content">{t("lightbox.type")}:</strong> {selected.kind}</span>
        <span><strong className="font-semibold text-base-content">{t("lightbox.size")}:</strong> {selected.size_bytes === null ? "-" : formatBytes(selected.size_bytes)}</span>
        <span>
          <strong className="font-semibold text-base-content">{t("lightbox.dimensions")}:</strong> {selected.width ?? "-"} x {selected.height ?? "-"}
        </span>
        <span><strong className="font-semibold text-base-content">{t("lightbox.duration")}:</strong> {formatDuration(selected.duration_ms)}</span>
        <span className="break-all text-[11px] leading-relaxed text-base-content/60">{t("lightbox.path")}: {selected.path ?? selected.file_name}</span>
      </div>
    </aside>
  );
}