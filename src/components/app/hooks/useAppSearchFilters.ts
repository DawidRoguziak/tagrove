import { useCallback, useMemo, useState } from "react";
import type { SearchFilters, SearchMediaKind } from "../types";
import {
  areSearchFiltersEqual,
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

  const applyMediaKindAndSubmit = useCallback(
    async (value: SearchMediaKind, refresh?: () => Promise<void> | void) => {
      setMediaKind(value);
      const nextFilters: SearchFilters = {
        filterInput,
        mediaKind: value,
        favoritesOnly
      };

      if (areSearchFiltersEqual(nextFilters, appliedFilters)) {
        await refresh?.();
        return;
      }

      setAppliedFilterInput(filterInput);
      setAppliedMediaKind(value);
      setAppliedFavoritesOnly(favoritesOnly);
    },
    [appliedFilters, favoritesOnly, filterInput]
  );

  const applyFavoritesOnlyAndSubmit = useCallback(
    async (value: boolean, refresh?: () => Promise<void> | void) => {
      setFavoritesOnly(value);
      const nextFilters: SearchFilters = {
        filterInput,
        mediaKind,
        favoritesOnly: value
      };

      if (areSearchFiltersEqual(nextFilters, appliedFilters)) {
        await refresh?.();
        return;
      }

      setAppliedFilterInput(filterInput);
      setAppliedMediaKind(mediaKind);
      setAppliedFavoritesOnly(value);
    },
    [appliedFilters, filterInput, mediaKind]
  );

  const handleSearchSubmit = useCallback(
    async (refresh?: () => Promise<void> | void) => {
      const nextParsedFilter = parseSearchFilter(filterInput);
      if (nextParsedFilter.validationError) {
        setFilterValidationError(nextParsedFilter.validationError);
        return;
      }

      setFilterValidationError(null);

      if (areSearchFiltersEqual(currentFilters, appliedFilters)) {
        await refresh?.();
        return;
      }

      setAppliedFilterInput(filterInput);
      setAppliedMediaKind(mediaKind);
      setAppliedFavoritesOnly(favoritesOnly);
    },
    [appliedFilters, currentFilters, favoritesOnly, filterInput, mediaKind]
  );

  const applyFilterInputAndSubmit = useCallback(
    async (nextFilterInput: string, refresh?: () => Promise<void> | void) => {
      const nextParsedFilter = parseSearchFilter(nextFilterInput);
      if (nextParsedFilter.validationError) {
        setFilterValidationError(nextParsedFilter.validationError);
        return;
      }

      setFilterValidationError(null);
      setFilterInput(nextFilterInput);

      const nextFilters: SearchFilters = {
        filterInput: nextFilterInput,
        mediaKind,
        favoritesOnly
      };

      if (areSearchFiltersEqual(nextFilters, appliedFilters)) {
        await refresh?.();
        return;
      }

      setAppliedFilterInput(nextFilterInput);
      setAppliedMediaKind(mediaKind);
      setAppliedFavoritesOnly(favoritesOnly);
    },
    [appliedFilters, favoritesOnly, mediaKind]
  );

  const handleClearSearch = useCallback(
    async (refresh?: () => Promise<void> | void) => {
      setFilterInput(DEFAULT_SEARCH_FILTERS.filterInput);
      setMediaKind(DEFAULT_SEARCH_FILTERS.mediaKind);
      setFavoritesOnly(DEFAULT_SEARCH_FILTERS.favoritesOnly);
      setFilterValidationError(null);

      if (areSearchFiltersEqual(appliedFilters, DEFAULT_SEARCH_FILTERS)) {
        await refresh?.();
        return;
      }

      setAppliedFilterInput(DEFAULT_SEARCH_FILTERS.filterInput);
      setAppliedMediaKind(DEFAULT_SEARCH_FILTERS.mediaKind);
      setAppliedFavoritesOnly(DEFAULT_SEARCH_FILTERS.favoritesOnly);
    },
    [appliedFilters]
  );

  return {
    filterInput,
    mediaKind,
    favoritesOnly,
    appliedMediaKind,
    appliedFavoritesOnly,
    appliedParsedFilter,
    appliedMetaFilterKey,
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
