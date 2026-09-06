import { useCallback, useState, type Dispatch, type SetStateAction } from "react";
import type { AssetDetails, AssetSummary, SelectedAsset } from "../types";
import type { AssetTagStateController } from "../components/app/hooks/useAssetTagState";
import { toggleLightboxFavoriteAction } from "../components/lightbox/services/toggleLightboxFavoriteAction";
import { saveLightboxMediaGroupAction } from "../components/lightbox/services/saveLightboxMediaGroupAction";
type Status = { id: number; pending: boolean; failed: boolean } | null;
interface Options {
  selected: SelectedAsset | null;
  assetTagState: AssetTagStateController;
  appliedFavoritesOnly: boolean;
  setAssets: Dispatch<SetStateAction<AssetSummary[]>>;
  setSelectedState: Dispatch<SetStateAction<SelectedAsset | null>>;
  refresh: () => Promise<void>;
  getCachedDetails: (id: number) => AssetDetails | null;
  putCachedDetails: (details: AssetDetails) => void;
}
export function useSelectionMetadataActions({
  selected,
  assetTagState,
  appliedFavoritesOnly,
  setAssets,
  setSelectedState,
  refresh,
  getCachedDetails,
  putCachedDetails
}: Options) {
  const [favoriteStatus, setFavoriteStatus] = useState<Status>(null);
  const [groupStatus, setGroupStatus] = useState<Status>(null);
  const toggleSelectedFavorite = useCallback(async () => {
    if (!selected) return;
    const id = selected.id;
    const mutationToken = assetTagState.beginMutation(selected.id);
    if (!mutationToken) {
      setFavoriteStatus({ id, pending: false, failed: true });
      throw new Error("Metadata is busy. Retry after the current operation.");
    }
    setFavoriteStatus({ id, pending: true, failed: false });
    try {
      await toggleLightboxFavoriteAction({
        selected,
        appliedFavoritesOnly,
        setAssets,
        setSelected: setSelectedState,
        refresh
      });
      const cached = getCachedDetails(selected.id);
      if (cached) putCachedDetails({ ...cached, is_favorite: !selected.is_favorite });
    } catch (error) {
      setFavoriteStatus({ id, pending: false, failed: true });
      throw error;
    } finally {
      setFavoriteStatus((previous) =>
        previous?.id === id ? { ...previous, pending: false } : previous
      );
      assetTagState.settleMutation(mutationToken);
    }
  }, [
    appliedFavoritesOnly,
    assetTagState,
    getCachedDetails,
    putCachedDetails,
    refresh,
    selected,
    setAssets,
    setSelectedState
  ]);

  const saveMediaGroup = useCallback(
    async (next: { key: string | null; order: number | null }) => {
      if (!selected) return;
      const id = selected.id;
      const mutationToken = assetTagState.beginMutation(selected.id);
      if (!mutationToken) {
        setGroupStatus({ id, pending: false, failed: true });
        throw new Error("Metadata is busy. Retry after the current operation.");
      }
      setGroupStatus({ id, pending: true, failed: false });
      try {
        await saveLightboxMediaGroupAction(
          { selected, setAssets, setSelected: setSelectedState },
          next
        );
        const cached = getCachedDetails(selected.id);
        if (cached) {
          putCachedDetails({
            ...cached,
            media_group_key: next.key,
            media_group_order: next.order
          });
        }
        // Media-group changes always affect grouping/ordering, so start a new
        // session instead of relying on the local patch.
        void refresh().catch(() => {});
      } catch (error) {
        setGroupStatus({ id, pending: false, failed: true });
        throw error;
      } finally {
        setGroupStatus((previous) =>
          previous?.id === id ? { ...previous, pending: false } : previous
        );
        assetTagState.settleMutation(mutationToken);
      }
    },
    [
      assetTagState,
      getCachedDetails,
      putCachedDetails,
      refresh,
      selected,
      setAssets,
      setSelectedState
    ]
  );

  return {
    toggleSelectedFavorite,
    saveMediaGroup,
    favoriteFailed: favoriteStatus?.id === selected?.id && favoriteStatus?.failed === true,
    groupFailed: groupStatus?.id === selected?.id && groupStatus?.failed === true,
    favoritePending: favoriteStatus?.id === selected?.id && favoriteStatus?.pending === true,
    groupPending: groupStatus?.id === selected?.id && groupStatus?.pending === true
  };
}
