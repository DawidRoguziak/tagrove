import { createMemo, merge, omit } from "solid-js";
import type { JSX } from "@solidjs/web";

interface UiAlertProps extends JSX.HTMLAttributes<HTMLDivElement> {
  className?: string;
  tone?: "info" | "warning" | "error";
  title?: string;
  children: JSX.Element;
}

export function UiAlert(input: UiAlertProps) {
  const props = merge({ tone: "info", className: "" }, input);
  const rest = omit(props, "tone", "title", "className", "children");

  const classes = createMemo(() => {
    const { tone, className } = props;
    return [
      "rounded-[var(--radius-control)] border px-3.5 py-3 text-base-content",
      tone === "warning"
        ? "border-warning/52 bg-warning/14"
        : tone === "error"
          ? "border-error/52 bg-error/14"
          : "border-info/52 bg-info/14",
      className
    ]
      .filter(Boolean)
      .join(" ");
  });

  return (
    <div role="alert" class={classes()} {...rest}>
      {props.title ? (
        <p class="m-0 text-xs font-semibold leading-5 text-base-content">{props.title}</p>
      ) : null}
      <div class={`m-0 text-xs leading-5 text-base-content/78 ${props.title ? "mt-0.5" : ""}`}>
        {props.children}
      </div>
    </div>
  );
}
