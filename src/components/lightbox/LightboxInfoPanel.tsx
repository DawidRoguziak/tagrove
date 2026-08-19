import type { Asset } from "../../types";
import { formatBytes, formatDuration } from "../../utils/media";
import { UiIconButton } from "../UI/UiIconButton";
import { useTranslation } from "react-i18next";

interface LightboxInfoPanelProps {
  open: boolean;
  selected: Asset;
  onToggle: () => void;
}

export function LightboxInfoPanel({ open, selected, onToggle }: LightboxInfoPanelProps) {
  const { t } = useTranslation();

  return (
    <div className="relative">
      <UiIconButton
        icon="info"
        active={open}
        aria-label={t("lightbox.showInfo")}
        title={t("lightbox.info")}
        onClick={onToggle}
      />
      <aside
        className={`panel-scroll absolute z-[6] grid max-h-[min(380px,calc(100vh-140px))] w-[min(360px,calc(100vw-2.5rem))] content-start gap-3 overflow-auto rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-raised)] p-4 shadow-[var(--shadow-popover)] backdrop-blur-xl transition-[opacity,translate] duration-150 bottom-[calc(100%+10px)] right-0 lg:bottom-auto lg:right-[calc(100%+12px)] lg:top-1/2 lg:-translate-y-1/2 ${
          open
            ? "pointer-events-auto translate-y-0 opacity-100 lg:translate-y-[-50%]"
            : "pointer-events-none translate-y-2 opacity-0 lg:translate-y-[calc(-50%-6px)]"
        }`}
      >
        <h3 className="m-0 text-base">{t("lightbox.infoHeading")}</h3>
        <div className="grid gap-2 rounded-xl bg-base-200/45 p-3 text-[13px] text-base-content/82">
          <span><strong className="font-semibold text-base-content">{t("lightbox.type")}:</strong> {selected.kind}</span>
          <span><strong className="font-semibold text-base-content">{t("lightbox.size")}:</strong> {formatBytes(selected.size_bytes)}</span>
          <span>
            <strong className="font-semibold text-base-content">{t("lightbox.dimensions")}:</strong> {selected.width ?? "-"} x {selected.height ?? "-"}
          </span>
          <span><strong className="font-semibold text-base-content">{t("lightbox.duration")}:</strong> {formatDuration(selected.duration_ms)}</span>
          <span className="break-all text-xs leading-relaxed text-base-content/60">{t("lightbox.path")}: {selected.path}</span>
        </div>
      </aside>
    </div>
  );
}
