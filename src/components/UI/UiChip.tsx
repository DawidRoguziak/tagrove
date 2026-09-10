import type { HTMLAttributes } from "react";

interface UiChipProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: "default" | "video" | "gif";
}

export function UiChip({ tone = "default", className = "", ...props }: UiChipProps) {
  const classes = [
    "inline-flex items-center border px-2.5 py-1 text-xs font-medium leading-none",
    "rounded-[var(--radius-control)]",
    tone === "video"
      ? "border-white/20 bg-[var(--media-video-bg)] text-[var(--media-video-content)]"
      : tone === "gif"
        ? "border-white/20 bg-[var(--media-gif-bg)] text-[var(--media-gif-content)]"
        : "border-[var(--border-soft)] bg-[var(--surface-raised)] text-base-content",
    className
  ]
    .filter(Boolean)
    .join(" ");

  return <span className={classes} {...props} />;
}
