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
      const origin = group.getBoundingClientRect();
      const labels = Array.from(group.querySelectorAll("label"));
      const bounds = labels.map(option => {
        const rect = option.getBoundingClientRect();
        return [rect.left - origin.left - group.clientLeft, rect.top - origin.top - group.clientTop, rect.width, rect.height];
      });
      const selected = labels.findIndex(option => option.querySelector("input:checked"));
      const rect = bounds[selected];
      if (!rect) {
        highlight.hidden = true;
        previousRef.current = null;
        return;
      }
      const layout = JSON.stringify([bounds, labels.map(option => option.textContent)]);
      const target = JSON.stringify(rect);
      const previous = previousRef.current;
      // Initial observer notifications must not interrupt an in-flight selection.
      if (previous?.layout === layout && previous.target === target) return;
      const slide = animate && previous?.layout === layout;
      highlight.style.transition = slide ? "" : "none";
      highlight.hidden = false;
      const [left, top, width, height] = rect;
      highlight.style.transform = `translate(${left}px, ${top}px)`;
      highlight.style.width = `${width}px`;
      highlight.style.height = `${height}px`;
      // Commit immediate placement before a subsequent selection can transition.
      if (!slide) highlight.getBoundingClientRect();
      previousRef.current = { layout, target };
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
