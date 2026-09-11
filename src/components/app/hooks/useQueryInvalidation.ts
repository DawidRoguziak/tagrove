import { useCallback, useRef } from "react";
import type { TagQueryImpact } from "../../../types";
import type { FilterDescriptor } from "../services/filterService";
import { tagMutationRequiresRefresh } from "../services/libraryInvalidationService";

export type OnTagMutation = (impact: TagQueryImpact, missingIds?: boolean) => void;

/** Saves may finish after the user applies another query. Read its current state at commit. */
export function useQueryInvalidation(
  filter: FilterDescriptor | string[],
  refreshQuery: () => Promise<void>,
  favoritesOnly: boolean,
  onCommitted?: OnTagMutation
) {
  const latest = useRef({ filter, refreshQuery, favoritesOnly, onCommitted });
  latest.current = { filter, refreshQuery, favoritesOnly, onCommitted };
  const refresh = useCallback(() => latest.current.refreshQuery(), []);
  const isFavoritesOnly = useCallback(() => latest.current.favoritesOnly, []);
  const onTagMutation = useCallback<OnTagMutation>((impact, missingIds = false) => {
    const current = latest.current;
    if (current.onCommitted) current.onCommitted(impact, missingIds);
    else if (missingIds || tagMutationRequiresRefresh(impact, current.filter)) {
      void current.refreshQuery().catch(() => {});
    }
  }, []);
  return { refresh, isFavoritesOnly, onTagMutation };
}
