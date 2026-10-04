import type { SelectedAsset } from "../../types";
import { formatBytes, formatDuration } from "../../utils/media";
import { useLanguage } from "../../i18n/solid";

interface LightboxInfoPanelProps {
  selected: SelectedAsset;
}

export function LightboxInfoPanel(props: LightboxInfoPanelProps) {
  const { t } = useLanguage();

  return (
    <aside
      aria-label={t("lightbox.infoHeading")}
      data-testid="lightbox-info-panel"
      class="motion-enter grid gap-1.5 rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-[var(--surface-muted)] p-3 text-xs text-[var(--text-muted)]"
    >
      <h3 class="m-0 text-xs">{t("lightbox.infoHeading")}</h3>
      <div class="grid gap-2 font-mono text-xs">
        <span>
          <strong class="font-semibold text-base-content">{t("lightbox.type")}:</strong>{" "}
          {props.selected.kind}
        </span>
        {props.selected.size_bytes !== null ? (
          <span>
            <strong class="font-semibold text-base-content">{t("lightbox.size")}:</strong>{" "}
            {formatBytes(props.selected.size_bytes)}
          </span>
        ) : null}
        {props.selected.width !== null && props.selected.height !== null ? (
          <span>
            <strong class="font-semibold text-base-content">{t("lightbox.dimensions")}:</strong>{" "}
            {props.selected.width} x {props.selected.height}
          </span>
        ) : null}
        {props.selected.duration_ms !== null ? (
          <span>
            <strong class="font-semibold text-base-content">{t("lightbox.duration")}:</strong>{" "}
            {formatDuration(props.selected.duration_ms)}
          </span>
        ) : null}
        <span class="break-all text-xs leading-relaxed text-[var(--text-muted)]">
          {t("lightbox.path")}: {props.selected.path ?? props.selected.file_name}
        </span>
      </div>
    </aside>
  );
}
