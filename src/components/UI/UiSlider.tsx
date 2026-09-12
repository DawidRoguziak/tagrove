import type { InputHTMLAttributes } from "react";

type UiSliderProps = Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "min" | "max" | "value" | "defaultValue"> & {
  min: number;
  max: number;
  value: number;
};

export function UiSlider({ className = "", min, max, value, style, ...props }: UiSliderProps) {
  const progress = (value - min) / (max - min) * 100;
  const classes = [
    "m-0 h-[4px] w-full appearance-none rounded-full border-0 bg-[var(--border-strong)] p-0",
    "[&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border [&::-webkit-slider-thumb]:border-[oklch(var(--b1))] [&::-webkit-slider-thumb]:bg-primary [&::-webkit-slider-thumb]:shadow-[0_0_0_2px_oklch(var(--p)/0.24)]",
    "[&::-moz-range-thumb]:h-3 [&::-moz-range-thumb]:w-3 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:border [&::-moz-range-thumb]:border-[oklch(var(--b1))] [&::-moz-range-thumb]:bg-primary [&::-moz-range-thumb]:shadow-[0_0_0_2px_oklch(var(--p)/0.24)]",
    "[&::-moz-range-track]:h-[4px] [&::-moz-range-track]:rounded-full [&::-moz-range-track]:bg-transparent",
    className
  ]
    .filter(Boolean)
    .join(" ");
  return <input type="range" min={min} max={max} value={value} className={classes}
    style={{ backgroundImage: `linear-gradient(to right, var(--color-primary) ${progress}%, transparent ${progress}%)`, ...style }} {...props} />;
}
