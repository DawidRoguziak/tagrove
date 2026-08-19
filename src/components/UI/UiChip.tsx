import type { HTMLAttributes } from "react";

interface UiChipProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: "default" | "video" | "gif";
}

export function UiChip({ tone = "default", className = "", ...props }: UiChipProps) {
  const isMediaChip = tone === "video" || tone === "gif";
  const classes = [
    "inline-flex items-center border px-2.5 py-1 text-[10px] font-bold uppercase leading-none tracking-[0.1em] backdrop-blur-md",
    isMediaChip
      ? "rounded-none rounded-bl-[var(--radius-surface)] border-r-0 border-t-0"
      : "rounded-full",
    tone === "video"
      ? "border-accent/35 bg-accent/88 text-accent-content"
      : tone === "gif"
        ? "border-info/35 bg-info/88 text-info-content"
        : "border-[var(--border-soft)] bg-[var(--surface-raised)] text-base-content",
    className
  ]
    .filter(Boolean)
    .join(" ");

  return <span className={classes} {...props} />;
}
