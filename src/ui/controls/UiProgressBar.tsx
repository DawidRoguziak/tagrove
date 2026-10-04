interface UiProgressBarProps {
  value?: number;
  max: number;
  label?: string;
}

export function UiProgressBar(props: UiProgressBarProps) {
  return (
    <div class="flex w-full max-w-[420px] items-center gap-3">
      <progress
        class="progress progress-primary h-1.5 min-w-0 flex-1 rounded-[2px]"
        aria-label={props.label}
        value={props.value}
        max={props.max}
      />
      {props.label && <span class="text-xs text-base-content">{props.label}</span>}
    </div>
  );
}
