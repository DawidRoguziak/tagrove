import type { AssetDetails, AssetSummary } from "../../../platform/tauri/contracts/assets";
export function asset(id: number, changes: Partial<AssetDetails> = {}): AssetDetails {
  return {
    id,
    file_name: `${id}.png`,
    path: `/synthetic/${id}.png`,
    preview_path: null,
    size_bytes: 10,
    kind: "image",
    width: 100,
    height: 80,
    modified_at: 1,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    tags: ["cat"],
    ...changes
  };
}
export function summary(id: number, changes: Partial<AssetSummary> = {}): AssetSummary {
  const { path: _path, size_bytes: _size, tags: _tags, ...value } = asset(id);
  return { ...value, ...changes };
}
export function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (error: Error) => void;
  const promise = new Promise<T>((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
}
export const saved = (id: number, tags: string[]) => ({
  asset_id: id,
  tags,
  changed: true,
  query_impact: { type: "none" },
  vocabulary_changed: false,
  revision: 2
});
