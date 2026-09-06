import { parseSearchFilter } from "../../../utils/media";
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

export interface FilterDescriptor {
  include: string[];
  exclude: string[];
  metaFilter: SearchMetaFilter | null;
  mediaKind: SearchFilters["mediaKind"];
  favoritesOnly: boolean;
}

export function buildFilterDescriptor(filters: SearchFilters): FilterDescriptor {
  const parsed = parseSearchFilter(filters.filterInput);
  const normalize = (tags: string[]) => [...new Set(tags.map(tag => tag.trim().toLowerCase()))].sort();
  return {
    include: normalize(parsed.include),
    exclude: normalize(parsed.exclude),
    metaFilter: parsed.metaFilter?.type === "groupName"
      ? { ...parsed.metaFilter, groupName: parsed.metaFilter.groupName.trim().toLowerCase() }
      : parsed.metaFilter,
    mediaKind: filters.mediaKind,
    favoritesOnly: filters.favoritesOnly
  };
}

export function serializeFilterDescriptor(descriptor: FilterDescriptor): string {
  return JSON.stringify(descriptor);
}
