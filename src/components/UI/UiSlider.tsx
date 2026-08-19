import type { InputHTMLAttributes } from "react";

type UiSliderProps = Omit<InputHTMLAttributes<HTMLInputElement>, "type">;

export function UiSlider({ className = "", ...props }: UiSliderProps) {
  const classes = [
    "m-0 h-[3px] w-full appearance-none rounded-full border-0 bg-[linear-gradient(90deg,oklch(var(--p)/0.62),oklch(var(--s)/0.66))] p-0",
    "[&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border [&::-webkit-slider-thumb]:border-[oklch(var(--b1))] [&::-webkit-slider-thumb]:bg-primary [&::-webkit-slider-thumb]:shadow-[0_0_0_2px_oklch(var(--p)/0.24)]",
    "[&::-moz-range-thumb]:h-3 [&::-moz-range-thumb]:w-3 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:border [&::-moz-range-thumb]:border-[oklch(var(--b1))] [&::-moz-range-thumb]:bg-primary [&::-moz-range-thumb]:shadow-[0_0_0_2px_oklch(var(--p)/0.24)]",
    "[&::-moz-range-track]:h-[3px] [&::-moz-range-track]:rounded-full [&::-moz-range-track]:bg-[linear-gradient(90deg,oklch(var(--p)/0.62),oklch(var(--s)/0.66))]",
    className
  ]
    .filter(Boolean)
    .join(" ");
  return <input type="range" className={classes} {...props} />;
}
