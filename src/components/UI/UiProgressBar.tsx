interface UiProgressBarProps {
  value?: number;
  max: number;
  label?: string;
}

export function UiProgressBar({ value, max, label }: UiProgressBarProps) {
  return (
    <div className="flex items-center gap-2">
      <progress className="progress progress-primary h-2 w-[min(320px,70vw)]" value={value} max={max} />
      {label && <span className="text-xs text-base-content">{label}</span>}
    </div>
  );
}
