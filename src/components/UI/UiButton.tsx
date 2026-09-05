import type { ButtonHTMLAttributes } from "react";

interface UiButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "default" | "primary" | "secondary" | "ghost" | "danger";
}

export function UiButton({ variant = "secondary", className = "", type = "button", ...props }: UiButtonProps) {
  const classes = [
    "btn btn-sm h-9 min-h-9 rounded-[var(--radius-control)] border px-3 normal-case font-semibold tracking-[-0.01em] shadow-none transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-50",
    variant === "primary"
      ? "border-primary bg-primary text-primary-content hover:border-primary hover:bg-primary/88"
      : variant === "danger"
        ? "border-error/28 bg-error/8 text-error hover:border-error/50 hover:bg-error/16"
        : variant === "ghost"
          ? "border-transparent bg-transparent text-base-content/72 hover:bg-base-content/10 hover:text-base-content"
          : "border-[var(--border-soft)] bg-[var(--surface-solid)] text-base-content shadow-[var(--shadow-control)] hover:border-[var(--border-strong)] hover:bg-base-100",
    className
  ]
    .filter(Boolean)
    .join(" ");

  return <button type={type} className={classes} {...props} />;
}
