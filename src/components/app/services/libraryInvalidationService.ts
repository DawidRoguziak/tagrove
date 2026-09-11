import type { FilterDescriptor } from "./filterService";
import type { TagQueryImpact } from "../../../types";

export function tagMutationRequiresRefresh(impact: TagQueryImpact, filter: FilterDescriptor | string[]): boolean {
  switch (impact.type) {
    case "none": return false;
    case "all": return true;
    case "tags": {
      if (!Array.isArray(filter) && filter.metaFilter?.type === "hasNoTags" && impact.tag_count_changed) return true;
      const tags = Array.isArray(filter) ? filter : [...filter.include, ...filter.exclude];
      const applied = new Set(tags.map(tag => tag.trim().toLowerCase()));
      return impact.changed_tags.some(tag => applied.has(tag));
    }
    default: {
      const exhaustive: never = impact;
      return exhaustive;
    }
  }
}

export function favoriteMutationRequiresRefresh(appliedFavoritesOnly: boolean, nextFavorite: boolean): boolean {
  return appliedFavoritesOnly && !nextFavorite;
}
