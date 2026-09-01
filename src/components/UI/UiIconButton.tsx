import type { ButtonHTMLAttributes } from "react";
import { UiButton } from "./UiButton";
import { UiIcon, type UiIconName } from "./UiIcon";

interface UiIconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  icon: UiIconName;
  active?: boolean;
  danger?: boolean;
  iconClassName?: string;
}

export function UiIconButton({
  icon,
  active = false,
  danger = false,
  className = "",
  iconClassName = "h-5 w-5 shrink-0",
  style,
  ...props
}: UiIconButtonProps) {
  const classes = [
    "btn-circle h-10 w-10 min-h-10 shrink-0 p-0",
    active
      ? "bg-primary/16 text-primary ring-2 ring-primary/26"
      : "bg-[var(--surface-raised)] text-base-content/72 shadow-[var(--shadow-control)] hover:text-base-content",
    className
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <UiButton
      className={classes}
      variant={danger ? "danger" : "secondary"}
      style={{ ...style, padding: 0 }}
      {...props}
    >
      <UiIcon name={icon} filled={icon === "heart" && active} className={iconClassName} />
    </UiButton>
  );
}
