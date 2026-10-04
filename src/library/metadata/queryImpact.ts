import type { TagQueryImpact } from "../../platform/tauri/contracts/tags";
import type { SearchMetaFilter } from "../../platform/tauri/contracts/assets";
type FilterDescriptor = {
  include: string[];
  exclude: string[];
  metaFilter: SearchMetaFilter | null;
};

export function tagMutationRequiresRefresh(
  impact: TagQueryImpact,
  filter: FilterDescriptor
): boolean {
  switch (impact.type) {
    case "none":
      return false;
    case "all":
      return true;
    case "tags": {
      if (filter.metaFilter?.type === "hasNoTags" && impact.tag_count_changed)
        return true;
      const tags = [...filter.include, ...filter.exclude];
      const applied = new Set(tags.map((tag) => tag.trim().toLowerCase()));
      return impact.changed_tags.some((tag) => applied.has(tag));
    }
    default: {
      const exhaustive: never = impact;
      return exhaustive;
    }
  }
}

export function favoriteMutationRequiresRefresh(
  appliedFavoritesOnly: boolean,
  nextFavorite: boolean
): boolean {
  return appliedFavoritesOnly && !nextFavorite;
}
