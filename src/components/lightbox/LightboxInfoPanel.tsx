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
      className="motion-enter grid gap-1.5 rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-[var(--surface-muted)] p-3 text-xs text-[var(--text-muted)]"
    >
      <h3 className="m-0 text-xs">{t("lightbox.infoHeading")}</h3>
      <div className="grid gap-2 font-mono text-xs">
        <span><strong className="font-semibold text-base-content">{t("lightbox.type")}:</strong> {selected.kind}</span>
        {selected.size_bytes !== null ? <span><strong className="font-semibold text-base-content">{t("lightbox.size")}:</strong> {formatBytes(selected.size_bytes)}</span> : null}
        {selected.width !== null && selected.height !== null ? <span>
          <strong className="font-semibold text-base-content">{t("lightbox.dimensions")}:</strong> {selected.width} x {selected.height}
        </span> : null}
        {selected.duration_ms !== null ? <span><strong className="font-semibold text-base-content">{t("lightbox.duration")}:</strong> {formatDuration(selected.duration_ms)}</span> : null}
        <span className="break-all text-xs leading-relaxed text-[var(--text-muted)]">{t("lightbox.path")}: {selected.path ?? selected.file_name}</span>
      </div>
    </aside>
  );
}