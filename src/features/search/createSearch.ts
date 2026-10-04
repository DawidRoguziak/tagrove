import { createSignal } from "solid-js";
import { parseSearchFilter, type SearchFilterValidationError } from "../../utils/media";
import {
  DEFAULT_SEARCH_FILTERS,
  buildFilterDescriptor,
  serializeFilterDescriptor
} from "../../components/app/services/filterService";
import type { LibraryFilters } from "../../library/query/createAssetQuery";

type Filters = typeof DEFAULT_SEARCH_FILTERS;
export function createSearch(apply: (filters: LibraryFilters) => Promise<void>) {
  // Commands compose drafts synchronously, including multiple events in one Solid batch.
  let draft = { ...DEFAULT_SEARCH_FILTERS };
  const [state, publish] = createSignal(draft);
  const [applied, publishApplied] = createSignal(buildFilterDescriptor(draft));
  const [error, setError] = createSignal<SearchFilterValidationError | null>(null);
  let appliedKey = serializeFilterDescriptor(buildFilterDescriptor(draft));
  function update(next: Partial<Filters>) {
    draft = { ...draft, ...next };
    publish(draft);
    setError(null);
  }
  async function submit() {
    const parsed = parseSearchFilter(draft.filterInput);
    setError(parsed.validationError);
    if (parsed.validationError) return;
    const descriptor = buildFilterDescriptor(draft);
    const key = serializeFilterDescriptor(descriptor);
    if (key !== appliedKey) {
      appliedKey = key;
      publishApplied(descriptor);
    }
    await apply({
      filterInclude: descriptor.include,
      filterExclude: descriptor.exclude,
      metaFilter: descriptor.metaFilter,
      appliedMediaKind: descriptor.mediaKind,
      appliedFavoritesOnly: descriptor.favoritesOnly
    });
  }
  return {
    get filterInput() {
      return state().filterInput;
    },
    get mediaKind() {
      return state().mediaKind;
    },
    get favoritesOnly() {
      return state().favoritesOnly;
    },
    get applied() {
      return applied();
    },
    get validationError() {
      return error();
    },
    change: (filterInput: string) => update({ filterInput }),
    submit,
    applyInput: (filterInput: string) => {
      update({ filterInput });
      return submit();
    },
    applyKind: (mediaKind: Filters["mediaKind"]) => {
      update({ mediaKind });
      return submit();
    },
    applyFavorites: (favoritesOnly: boolean) => {
      update({ favoritesOnly });
      return submit();
    },
    clear: () => {
      update(DEFAULT_SEARCH_FILTERS);
      return submit();
    }
  };
}
