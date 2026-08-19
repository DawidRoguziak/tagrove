export const UI_FLUSH_DELAY_MS = 24;
export const QUEUE_PROCESS_DELAY_MS = 36;
export const THUMBNAIL_BATCH_SIZE = 64;

export function takeThumbnailBatch(source: Set<number>, size: number): number[] {
  const batch: number[] = [];
  for (const id of source) {
    batch.push(id);
    source.delete(id);
    if (batch.length >= size) {
      break;
    }
  }
  return batch;
}

export function mergeThumbnailUpdates(
  previous: Record<number, string>,
  pendingThumbUpdates: Record<number, string>
) {
  const next = { ...previous };
  let changed = false;

  for (const [assetIdText, thumbPath] of Object.entries(pendingThumbUpdates)) {
    const assetId = Number(assetIdText);
    if (next[assetId] === thumbPath) {
      continue;
    }

    next[assetId] = thumbPath;
    changed = true;
  }

  return changed ? next : previous;
}

export function removeRenderingAssetIds(
  previous: Record<number, true>,
  finishedRenderingIds: Set<number>
) {
  const next = { ...previous };
  let changed = false;

  for (const assetId of finishedRenderingIds) {
    if (!next[assetId]) {
      continue;
    }

    delete next[assetId];
    changed = true;
  }

  return changed ? next : previous;
}
