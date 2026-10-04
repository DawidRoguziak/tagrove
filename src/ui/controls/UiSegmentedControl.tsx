import { measureSegmentedHighlight } from "../dom/segmentedHighlight";
import { createEffect, createUniqueId, For } from "solid-js";

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

export function UiSegmentedControl<Value extends string>(props: UiSegmentedControlProps<Value>) {
  const name = createUniqueId();
  let groupRef: HTMLFieldSetElement | undefined;
  let highlightRef: HTMLDivElement | undefined;
  let previousMeasurement: { layout: string; target: string } | null = null;

  createEffect(
    () => [props.value, props.options] as const,
    () => {
      const group = groupRef;
      const highlight = highlightRef;
      if (!group || !highlight) return;

      function measure(animate: boolean) {
        if (!group || !highlight) return;
        previousMeasurement = measureSegmentedHighlight(
          group,
          highlight,
          previousMeasurement,
          animate
        );
      }

      measure(true);
      const observer = new ResizeObserver(() => measure(false));
      observer.observe(group);
      for (const option of group.querySelectorAll("label")) observer.observe(option);
      return () => observer.disconnect();
    }
  );

  return (
    <fieldset
      ref={(el) => {
        groupRef = el;
      }}
      id={props.id}
      class={`ui-segmented-control ${props.className ?? ""}`}
    >
      <legend class="sr-only">{props.label}</legend>
      <div
        ref={(el) => {
          highlightRef = el;
        }}
        class="ui-segmented-highlight"
        aria-hidden="true"
      />
      <For each={props.options}>
        {(option) => (
          <label>
            <input
              class="choice-input"
              type="radio"
              name={name}
              value={option.value}
              checked={props.value === option.value}
              onChange={() => {
                props.onChange(option.value);
                // Restore the committed selection until the parent's batched update arrives.
                for (const input of groupRef?.querySelectorAll("input") ?? [])
                  input.checked = input.value === props.value;
              }}
            />
            <span class="ui-segmented-label">
              <span class="ui-segmented-reserve" aria-hidden="true">
                {option.label}
              </span>
              <span>{option.label}</span>
            </span>
          </label>
        )}
      </For>
    </fieldset>
  );
}
