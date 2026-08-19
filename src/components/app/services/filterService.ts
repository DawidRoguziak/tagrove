import type { SearchFilters } from "../types";
import type { SearchMetaFilter } from "../../../types";

export const DEFAULT_SEARCH_FILTERS: SearchFilters = {
  filterInput: "",
  mediaKind: "all",
  favoritesOnly: false
};

export function areSearchFiltersEqual(left: SearchFilters, right: SearchFilters): boolean {
  return (
    left.filterInput === right.filterInput &&
    left.mediaKind === right.mediaKind &&
    left.favoritesOnly === right.favoritesOnly
  );
}

export function buildMetaFilterKey(metaFilter: SearchMetaFilter | null | undefined): string {
  if (!metaFilter) {
    return "";
  }

  return metaFilter.type === "hasNoTags"
    ? `hasNoTags:${metaFilter.tagCount}`
    : `groupName:${metaFilter.groupName.trim().toLowerCase()}`;
}
