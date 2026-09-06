import { useCallback, useMemo, useState } from "react";
import type { SearchFilters, SearchMediaKind } from "../types";
import {
  buildFilterDescriptor,
  serializeFilterDescriptor,
  buildMetaFilterKey,
  DEFAULT_SEARCH_FILTERS
} from "../services/filterService";
import {
  parseSearchFilter,
  type ParsedSearchFilter,
  type SearchFilterValidationError
} from "../../../utils/media";

interface UseAppSearchFiltersResult {
  filterInput: string;
  mediaKind: SearchMediaKind;
  favoritesOnly: boolean;
  appliedMediaKind: SearchMediaKind;
  appliedFavoritesOnly: boolean;
  appliedParsedFilter: ParsedSearchFilter;
  appliedMetaFilterKey: string;
  appliedDescriptor: ReturnType<typeof buildFilterDescriptor>;
  appliedQueryKey: string;
  filterValidationError: SearchFilterValidationError | null;
  setMediaKind: (value: SearchMediaKind) => void;
  setFavoritesOnly: (value: boolean) => void;
  applyMediaKindAndSubmit: (
    value: SearchMediaKind,
    refresh?: () => Promise<void> | void
  ) => Promise<void>;
  applyFavoritesOnlyAndSubmit: (
    value: boolean,
    refresh?: () => Promise<void> | void
  ) => Promise<void>;
  handleFilterChange: (value: string) => void;
  applyFilterInputAndSubmit: (
    nextFilterInput: string,
    refresh?: () => Promise<void> | void
  ) => Promise<void>;
  handleSearchSubmit: (refresh?: () => Promise<void> | void) => Promise<void>;
  handleClearSearch: (refresh?: () => Promise<void> | void) => Promise<void>;
}

export function useAppSearchFilters(): UseAppSearchFiltersResult {
  const [filterInput, setFilterInput] = useState(DEFAULT_SEARCH_FILTERS.filterInput);
  const [mediaKind, setMediaKind] = useState<SearchMediaKind>(DEFAULT_SEARCH_FILTERS.mediaKind);
  const [favoritesOnly, setFavoritesOnly] = useState(DEFAULT_SEARCH_FILTERS.favoritesOnly);
  const [appliedFilterInput, setAppliedFilterInput] = useState(DEFAULT_SEARCH_FILTERS.filterInput);
  const [appliedMediaKind, setAppliedMediaKind] = useState<SearchMediaKind>(
    DEFAULT_SEARCH_FILTERS.mediaKind
  );
  const [appliedFavoritesOnly, setAppliedFavoritesOnly] = useState(
    DEFAULT_SEARCH_FILTERS.favoritesOnly
  );
  const [filterValidationError, setFilterValidationError] =
    useState<SearchFilterValidationError | null>(null);

  const currentFilters = useMemo<SearchFilters>(
    () => ({ filterInput, mediaKind, favoritesOnly }),
    [favoritesOnly, filterInput, mediaKind]
  );
  const appliedFilters = useMemo<SearchFilters>(
    () => ({
      filterInput: appliedFilterInput,
      mediaKind: appliedMediaKind,
      favoritesOnly: appliedFavoritesOnly
    }),
    [appliedFavoritesOnly, appliedFilterInput, appliedMediaKind]
  );
  const appliedParsedFilter = useMemo(
    () => parseSearchFilter(appliedFilterInput),
    [appliedFilterInput]
  );
  const appliedMetaFilterKey = useMemo(
    () => buildMetaFilterKey(appliedParsedFilter.metaFilter),
    [appliedParsedFilter.metaFilter]
  );

  const handleFilterChange = useCallback((value: string) => {
    setFilterInput(value);
    setFilterValidationError(null);
  }, []);

  const appliedDescriptor = useMemo(() => buildFilterDescriptor(appliedFilters), [appliedFilters]);
  const appliedQueryKey = serializeFilterDescriptor(appliedDescriptor);

  const commit = useCallback(async (next: SearchFilters, refresh?: () => Promise<void> | void) => {
    const parsed = parseSearchFilter(next.filterInput);
    setFilterValidationError(parsed.validationError);
    if (parsed.validationError) return;
    if (serializeFilterDescriptor(buildFilterDescriptor(next)) === appliedQueryKey) {
      await refresh?.();
      return;
    }
    setAppliedFilterInput(next.filterInput);
    setAppliedMediaKind(next.mediaKind);
    setAppliedFavoritesOnly(next.favoritesOnly);
  }, [appliedQueryKey]);

  const applyMediaKindAndSubmit = useCallback(async (value: SearchMediaKind, refresh?: () => Promise<void> | void) => {
    setMediaKind(value);
    await commit({ ...currentFilters, mediaKind: value }, refresh);
  }, [commit, currentFilters]);

  const applyFavoritesOnlyAndSubmit = useCallback(async (value: boolean, refresh?: () => Promise<void> | void) => {
    setFavoritesOnly(value);
    await commit({ ...currentFilters, favoritesOnly: value }, refresh);
  }, [commit, currentFilters]);

  const handleSearchSubmit = useCallback(async (refresh?: () => Promise<void> | void) => {
    await commit(currentFilters, refresh);
  }, [commit, currentFilters]);

  const applyFilterInputAndSubmit = useCallback(async (nextFilterInput: string, refresh?: () => Promise<void> | void) => {
    setFilterInput(nextFilterInput);
    await commit({ ...currentFilters, filterInput: nextFilterInput }, refresh);
  }, [commit, currentFilters]);

  const handleClearSearch = useCallback(async (refresh?: () => Promise<void> | void) => {
    setFilterInput(DEFAULT_SEARCH_FILTERS.filterInput);
    setMediaKind(DEFAULT_SEARCH_FILTERS.mediaKind);
    setFavoritesOnly(DEFAULT_SEARCH_FILTERS.favoritesOnly);
    await commit(DEFAULT_SEARCH_FILTERS, refresh);
  }, [commit]);

  return {
    filterInput,
    mediaKind,
    favoritesOnly,
    appliedMediaKind,
    appliedFavoritesOnly,
    appliedParsedFilter,
    appliedMetaFilterKey,
    appliedDescriptor,
    appliedQueryKey,
    filterValidationError,
    setMediaKind,
    setFavoritesOnly,
    applyMediaKindAndSubmit,
    applyFavoritesOnlyAndSubmit,
    handleFilterChange,
    applyFilterInputAndSubmit,
    handleSearchSubmit,
    handleClearSearch
  };
}
