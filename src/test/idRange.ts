/**
 * Test doubles for the binary `get_asset_query_id_range` command.
 * - `idRangeResult` is what the mocked `getAssetQueryIdRange` wrapper resolves.
 * - `idRangeBuffer` is the raw body a mocked `invoke` resolves (f64 LE
 *   `[count, ...ids]`); an empty `ArrayBuffer` stands for `stale`.
 */
export function idRangeResult(ids: ArrayLike<number>) {
  return { status: "ready" as const, ids: Float64Array.from(ids) };
}

export function idRangeBuffer(ids: ArrayLike<number>): ArrayBuffer {
  const values = new Float64Array(ids.length + 1);
  values[0] = ids.length;
  values.set(Array.from(ids), 1);
  return values.buffer;
}

/** Inclusive `start..=end` IDs where the ID at index `i` is `i + offset`. */
export function sequentialIds(start: number, end: number, offset = 1) {
  return Array.from({ length: Math.max(0, end - start + 1) }, (_, i) => start + i + offset);
}
