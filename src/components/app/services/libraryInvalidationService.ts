import type { FilterDescriptor } from "./filterService";
function normalizeTag(tag: string): string {
  return tag.trim().toLowerCase();
}

export function collectChangedTags(previous: string[], next: string[]): string[] {
  const previousSet = new Set(previous.map(normalizeTag));
  const nextSet = new Set(next.map(normalizeTag));
  const changed: string[] = [];
  for (const tag of previous) {
    if (!nextSet.has(normalizeTag(tag))) changed.push(tag);
  }
  for (const tag of next) {
    if (!previousSet.has(normalizeTag(tag)) && !changed.some((existing) => normalizeTag(existing) === normalizeTag(tag))) {
      changed.push(tag);
    }
  }
  return changed;
}

export function tagMutationTouchesFilters(changedTags: string[], filter: FilterDescriptor | string[]): boolean {
  const appliedFilterTags = Array.isArray(filter) ? filter : [...filter.include, ...filter.exclude];
  if (!Array.isArray(filter) && filter.metaFilter?.type === "hasNoTags") return changedTags.length > 0;
  if (changedTags.length === 0 || appliedFilterTags.length === 0) return false;
  const applied = new Set(appliedFilterTags.map(normalizeTag));
  return changedTags.some((tag) => applied.has(normalizeTag(tag)));
}

export function favoriteMutationRequiresRefresh(appliedFavoritesOnly: boolean, nextFavorite: boolean): boolean {
  return appliedFavoritesOnly && !nextFavorite;
}

export function bulkTagMutationRequiresRefresh(updatedAssets: number, filter: FilterDescriptor | string[]): boolean {
  return updatedAssets > 0 && (Array.isArray(filter) ? filter.length > 0 : filter.include.length > 0 || filter.exclude.length > 0 || filter.metaFilter?.type === "hasNoTags");
}
