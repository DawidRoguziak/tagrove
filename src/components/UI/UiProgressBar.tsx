interface UiProgressBarProps {
  value?: number;
  max: number;
  label?: string;
}

export function UiProgressBar({ value, max, label }: UiProgressBarProps) {
  return (
    <div className="flex w-full max-w-[420px] items-center gap-3">
      <progress className="progress progress-primary h-1.5 min-w-0 flex-1 rounded-[2px]" aria-label={label} value={value} max={max} />
      {label && <span className="text-xs text-base-content">{label}</span>}
    </div>
  );
}
