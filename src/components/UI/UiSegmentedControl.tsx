import { measureSegmentedHighlight } from "../../ui/dom/segmentedHighlight";
import { useId, useLayoutEffect, useRef } from "react";

export interface UiSegmentedOption<Value extends string> {
  value: Value;
  label: string;
}

interface UiSegmentedControlProps<Value extends string> {
  options: readonly UiSegmentedOption<Value>[];
  value: NoInfer<Value>;
  onChange: (value: Value) => void;
  label: string;
  id?: string;
  className?: string;
}

export function UiSegmentedControl<Value extends string>({
  options, value, onChange, label, id, className = ""
}: UiSegmentedControlProps<Value>) {
  const name = useId();
  const groupRef = useRef<HTMLFieldSetElement>(null);
  const highlightRef = useRef<HTMLDivElement>(null);
  const previousRef = useRef<{ layout: string; target: string } | null>(null);

  useLayoutEffect(() => {
    const group = groupRef.current;
    const highlight = highlightRef.current;
    if (!group || !highlight) return;

    function measure(animate: boolean) {
      if (!group || !highlight) return;
      previousRef.current = measureSegmentedHighlight(group, highlight, previousRef.current, animate);
    }

    measure(true);
    const observer = new ResizeObserver(() => measure(false));
    observer.observe(group);
    for (const option of group.querySelectorAll("label")) observer.observe(option);
    return () => observer.disconnect();
  }, [value, options]);

  return (
    <fieldset ref={groupRef} id={id} className={`ui-segmented-control ${className}`}>
      <legend className="sr-only">{label}</legend>
      <div ref={highlightRef} className="ui-segmented-highlight" aria-hidden="true" />
      {options.map(option => (
        <label key={option.value}>
          <input className="choice-input" type="radio" name={name} value={option.value}
            checked={value === option.value} onChange={() => onChange(option.value)} />
          <span className="ui-segmented-label">
            <span className="ui-segmented-reserve" aria-hidden="true">{option.label}</span>
            <span>{option.label}</span>
          </span>
        </label>
      ))}
    </fieldset>
  );
}
