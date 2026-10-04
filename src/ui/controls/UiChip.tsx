import { createMemo, merge, omit } from "solid-js";
import type { JSX } from "@solidjs/web";

interface UiChipProps extends JSX.HTMLAttributes<HTMLSpanElement> {
  className?: string;
  tone?: "default" | "video" | "gif";
}

export function UiChip(input: UiChipProps) {
  const props = merge({ tone: "default", className: "" }, input);
  const rest = omit(props, "tone", "className");

  const classes = createMemo(() => {
    const { tone, className } = props;
    return [
      "inline-flex items-center border px-2 py-1 font-mono text-xs font-medium leading-none",
      "rounded-[var(--radius-tile)]",
      tone === "video"
        ? "border-white/20 bg-[var(--media-video-bg)] text-[var(--media-video-content)]"
        : tone === "gif"
          ? "border-white/20 bg-[var(--media-gif-bg)] text-[var(--media-gif-content)]"
          : "border-[var(--border-soft)] bg-[var(--surface-raised)] text-base-content",
      className
    ]
      .filter(Boolean)
      .join(" ");
  });

  return <span class={classes()} {...rest} />;
}
