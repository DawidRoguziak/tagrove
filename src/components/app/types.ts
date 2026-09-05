import type { GalleryRange } from "../gallery/hooks/useGalleryVirtualGrid";
import type { RefObject } from "react";
import type { AssetSummary } from "../../types";
import type { SearchFilterValidationError } from "../../utils/media";
import type { BulkSelectionInteraction } from "../gallery/GalleryGrid";
import type { ThumbnailStore } from "../../hooks/services/thumbnailStore";
export type { AppLanguage } from "../../i18n/languages";

export type SearchMediaKind = "all" | "image" | "gif" | "video";

export type AppTheme = "light" | "dark";

export interface SearchFilters {
  filterInput: string;
  mediaKind: SearchMediaKind;
  favoritesOnly: boolean;
}

export interface AppGallerySearchController {
  filterInput: string;
  onFilterChange: (value: string) => void;
  filterValidationError?: SearchFilterValidationError | null;
  knownTags: string[];
  mediaKind: SearchMediaKind;
  onMediaKindChange: (value: SearchMediaKind) => void | Promise<void>;
  onSearchSubmit: () => void;
  onApplyTagListSearch: (filterInput: string) => Promise<void> | void;
  onClearSearch: () => void;
  favoritesOnly: boolean;
  onFavoritesOnlyChange: (value: boolean) => void | Promise<void>;
}

export interface AppGalleryMediaController {
  assets: AssetSummary[];
  assetCount: number;
  getAssetAt: (index: number) => AssetSummary | undefined;
  selectedId: number | null;
  thumbs: Record<number, string>;
  tileSize: number;
  tileSizeMin: number;
  tileSizeMax: number;
  tileSizeStep: number;
  hasMore: boolean;
  isLoading: boolean;
  isGeneratingThumbnails: boolean;
  pendingThumbnailCount: number;
  renderingThumbnailIds: Record<number, true>;
  thumbnailStore: ThumbnailStore;
  scrollContainerRef: RefObject<HTMLElement | null>;
  onReachEnd: () => void;
  onVirtualRangeChange: (range: GalleryRange) => void;
  onCtrlWheelZoom: (deltaY: number) => void;
  onTileSizeChange: (nextSize: number) => void;
  onSelect: (asset: AssetSummary) => void;
  hasScanRoots: boolean;
  onAddFirstFolder: () => void;
  loadError: string | null;
  onLoadRetry: () => void;
  pageFailureEpoch: number;
}

export interface BulkSelectionController {
  selectionModeEnabled: boolean;
  selectedAssetIds: Set<number>;
  selectedAssets: AssetSummary[];
  knownTags: string[];
  groupKeyDraft: string;
  orderedAssetIds: number[];
  hasConflictingGroups: boolean;
  groupApplying: boolean;
  groupFailed: boolean;
  tagMode: "none" | "single" | "multiple";
  singleAssetTags: string[];
  appliedBulkTags: string[];
  tagDetailsLoading: boolean;
  tagDetailsFailed: boolean;
  tagApplying: boolean;
  tagSaveFailed: boolean;
  onToggleSelectionMode: () => void;
  onBulkSelectionInteraction: (interaction: BulkSelectionInteraction) => void;
  onGroupKeyDraftChange: (value: string) => void;
  onReorderGroupAsset: (draggedAssetId: number, targetAssetId: number) => void;
  onApplyGroup: () => Promise<void>;
  onAddTag: (tag: string) => Promise<boolean>;
  onRemoveTag: (tag: string) => Promise<void>;
  onRetryTagDetails: () => void;
}
