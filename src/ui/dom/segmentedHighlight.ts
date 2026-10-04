export interface HighlightMeasurement {
  layout: string;
  target: string;
}
export function measureSegmentedHighlight(
  group: HTMLElement,
  highlight: HTMLElement,
  previous: HighlightMeasurement | null,
  animate: boolean
): HighlightMeasurement | null {
  const origin = group.getBoundingClientRect();
  const labels = Array.from(group.querySelectorAll("label"));
  const bounds = labels.map((option) => {
    const rect = option.getBoundingClientRect();
    return [
      rect.left - origin.left - group.clientLeft,
      rect.top - origin.top - group.clientTop,
      rect.width,
      rect.height
    ];
  });
  const selected = labels.findIndex((option) => option.querySelector("input:checked"));
  const rect = bounds[selected];
  if (!rect) {
    highlight.hidden = true;
    return null;
  }
  const layout = JSON.stringify([bounds, labels.map((option) => option.textContent)]);
  const target = JSON.stringify(rect);
  // Initial observer notifications must not interrupt an in-flight selection.
  if (previous?.layout === layout && previous.target === target) return previous;
  const slide = animate && previous?.layout === layout;
  highlight.style.transition = slide ? "" : "none";
  highlight.hidden = false;
  const [left, top, width, height] = rect;
  highlight.style.transform = `translate(${left}px, ${top}px)`;
  highlight.style.width = `${width}px`;
  highlight.style.height = `${height}px`;
  // Commit immediate placement before a subsequent selection can transition.
  if (!slide) highlight.getBoundingClientRect();
  return { layout, target };
}
