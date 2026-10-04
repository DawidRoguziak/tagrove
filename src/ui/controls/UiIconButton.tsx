import { createMemo, merge, omit } from "solid-js";
import type { JSX } from "@solidjs/web";
import { UiButton } from "./UiButton";
import { UiIcon, type UiIconName } from "./UiIcon";

interface UiIconButtonProps
  extends Omit<JSX.ButtonHTMLAttributes<HTMLButtonElement>, "children" | "style"> {
  className?: string;
  style?: JSX.CSSProperties;
  icon: UiIconName;
  active?: boolean;
  danger?: boolean;
  iconClassName?: string;
}

export function UiIconButton(input: UiIconButtonProps) {
  const props = merge(
    { active: false, danger: false, className: "", iconClassName: "h-5 w-5 shrink-0" },
    input
  );
  const rest = omit(props, "icon", "active", "danger", "className", "iconClassName", "style");

  const classes = createMemo(() => {
    const { active, danger, icon, className } = props;
    return [
      "h-9 w-9 min-h-9 shrink-0 p-0",
      active && !danger
        ? icon === "heart"
          ? "border-favorite/40! bg-favorite/12! text-favorite!"
          : "border-primary/40! bg-primary/12! text-primary-text!"
        : "",
      className
    ]
      .filter(Boolean)
      .join(" ");
  });

  return (
    <UiButton
      className={classes()}
      variant={props.danger ? "danger" : "secondary"}
      style={{ ...props.style, padding: "0" }}
      {...rest}
    >
      <UiIcon
        name={props.icon}
        filled={props.icon === "heart" && props.active}
        className={props.iconClassName}
      />
    </UiButton>
  );
}
