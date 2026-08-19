import type { HTMLAttributes, ReactNode } from "react";

interface UiAlertProps extends HTMLAttributes<HTMLDivElement> {
  tone?: "info" | "warning" | "error";
  title?: string;
  children: ReactNode;
}

export function UiAlert({ tone = "info", title, className = "", children, ...props }: UiAlertProps) {
  const classes = [
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

  return (
    <div role="alert" className={classes} {...props}>
      {title ? <p className="m-0 text-xs font-semibold leading-5 text-base-content">{title}</p> : null}
      <p className={`m-0 text-xs leading-5 text-base-content/78 ${title ? "mt-0.5" : ""}`}>{children}</p>
    </div>
  );
}
