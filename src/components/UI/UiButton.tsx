import type { ButtonHTMLAttributes } from "react";

interface UiButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "default" | "primary" | "secondary" | "ghost" | "danger";
}

export function UiButton({ variant = "secondary", className = "", type = "button", ...props }: UiButtonProps) {
  const classes = [
    "btn btn-sm h-10 min-h-10 rounded-[var(--radius-control)] border px-4 normal-case font-semibold tracking-[-0.01em] shadow-none transition-[background-color,border-color,color,transform,box-shadow] duration-150 active:translate-y-px disabled:cursor-not-allowed disabled:opacity-50",
    variant === "primary"
      ? "border-primary bg-primary text-primary-content hover:border-primary hover:bg-primary/88"
      : variant === "danger"
        ? "border-error/28 bg-error/8 text-error hover:border-error/50 hover:bg-error/16"
        : variant === "ghost"
          ? "border-transparent bg-transparent text-base-content/72 hover:bg-base-content/10 hover:text-base-content"
          : "border-[var(--border-soft)] bg-[var(--surface-raised)] text-base-content shadow-[var(--shadow-control)] hover:border-primary/28 hover:bg-base-100",
    className
  ]
    .filter(Boolean)
    .join(" ");

  return <button type={type} className={classes} {...props} />;
}
