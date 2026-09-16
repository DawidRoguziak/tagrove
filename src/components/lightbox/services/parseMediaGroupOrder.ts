/** Empty clears the order; undefined rejects the draft without changing it. */
export function parseMediaGroupOrder(draft: string): number | null | undefined {
  if (draft === "") return null;
  if (/[^0-9]/.test(draft)) return undefined;
  const order = Number(draft);
  return Number.isSafeInteger(order) && order > 0 ? order : undefined;
}
