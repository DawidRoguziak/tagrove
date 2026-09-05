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
    "h-9 w-9 min-h-9 shrink-0 p-0",
    active && !danger ? "border-primary/40! bg-primary/12! text-primary!" : "",
    className
  ].filter(Boolean).join(" ");

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
