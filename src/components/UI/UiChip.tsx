import type { HTMLAttributes } from "react";

interface UiChipProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: "default" | "video" | "gif";
}

export function UiChip({ tone = "default", className = "", ...props }: UiChipProps) {
  const isMediaChip = tone === "video" || tone === "gif";
  const classes = [
    "inline-flex items-center border px-2.5 py-1 text-[10px] font-bold uppercase leading-none tracking-[0.1em]",
    isMediaChip
      ? "rounded-none rounded-bl-[var(--radius-surface)] border-r-0 border-t-0"
      : "rounded-[var(--radius-control)]",
    tone === "video"
      ? "border-white/20 bg-[#141618]/90 text-[#edf0f2]"
      : tone === "gif"
        ? "border-white/20 bg-[#141618]/90 text-[#edf0f2]"
        : "border-[var(--border-soft)] bg-[var(--surface-raised)] text-base-content",
    className
  ]
    .filter(Boolean)
    .join(" ");

  return <span className={classes} {...props} />;
}
