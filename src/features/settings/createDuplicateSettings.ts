import { createSignal } from "solid-js";
import { useLanguage } from "../../i18n/solid";
import {
  applyDuplicateResolutionBatch,
  findDuplicateAssets
} from "../../platform/tauri/maintenance";
import type { DuplicateScanSummary, DuplicateResolutionBatchChange } from "../../platform/tauri/contracts/maintenance";
import { isDuplicateScanProgressPhase } from "./progressService";
import type {
  DuplicatesSettingsController,
  DuplicateResolutionChange,
  PendingDuplicateDeleteConfirm
} from "./types";
import type { createSettingsOperationRunner } from "./createSettingsOperationRunner";
interface Options {
  runner: ReturnType<typeof createSettingsOperationRunner>;
  refreshLibrary: () => Promise<void>;
}
export function createDuplicateSettings({
  runner,
  refreshLibrary
}: Options): DuplicatesSettingsController {
  const { t } = useLanguage();
  const [duplicateDialogOpen, setDuplicateDialogOpen] = createSignal(false);
  const [pendingDuplicateDeleteConfirm, publishPendingDuplicateDeleteConfirm] =
    createSignal<PendingDuplicateDeleteConfirm | null>(null);
  let pendingDelete: PendingDuplicateDeleteConfirm | null = null;
  function setPendingDuplicateDeleteConfirm(value: PendingDuplicateDeleteConfirm | null) {
    pendingDelete = value;
    publishPendingDuplicateDeleteConfirm(value);
  }
  let latest: DuplicateScanSummary = {
    groups: [],
    duplicate_assets: 0,
    duplicate_groups: 0,
    revision: 0
  };
  const [scanResult, setScanResult] = createSignal(latest);
  function publishScan(summary: DuplicateScanSummary) {
    latest = summary;
    setScanResult(summary);
  }
  const onRescan = async () => {
    await runner.runExclusiveOperation(
      {
        section: "duplicates",
        pendingMessage: () => t("settings.actions.pending.findDuplicates"),
        errorPrefix: () => t("settings.actions.errorPrefix.duplicateScan"),
        progressPhaseMatcher: isDuplicateScanProgressPhase,
        setGlobalLoading: false
      },
      async () => {
        const summary = await findDuplicateAssets();
        publishScan(summary);
        runner.setSectionMessage("duplicates", () =>
          t("settings.actions.summary.duplicateScanComplete", {
            groups: summary.duplicate_groups,
            assets: summary.duplicate_assets
          })
        );
      }
    );
  };
  const onStart = async () => {
    setDuplicateDialogOpen(true);
    await onRescan();
  };
  const runSaveDuplicateChanges = async (changes: DuplicateResolutionChange[]) => {
    await runner.runExclusiveOperation(
      {
        section: "duplicates",
        pendingMessage: () => t("settings.actions.pending.applyDuplicateChanges"),
        errorPrefix: () => t("settings.actions.errorPrefix.saveDuplicateChanges"),
        setGlobalLoading: false
      },
      async () => {
        const assetsById = new Map(
          latest.groups.flatMap((group) => group.assets).map((asset) => [asset.id, asset])
        );
        const batchChanges: DuplicateResolutionBatchChange[] = changes.map((change) => {
          const newFileName = change.type === "rename" ? change.nextFileName.trim() : null;
          if (change.type === "rename" && !newFileName) {
            throw new Error(t("validation.fileNameCannotBeEmpty"));
          }
          const asset = assetsById.get(change.assetId);
          if (!asset)
            throw new Error(t("validation.unknownAssetInChanges", { assetId: change.assetId }));
          const common = {
            assetId: asset.id,
            expectedPath: asset.path,
            expectedRecordVersion: asset.record_version
          };
          if (change.type === "rename") {
            return { type: "rename", ...common, newFileName: change.nextFileName.trim() };
          }
          return { type: "delete", ...common };
        });
        const result = await applyDuplicateResolutionBatch(latest.revision, batchChanges);
        await refreshLibrary().catch(() => undefined);
        const duplicateSummary = await findDuplicateAssets().catch(() => null);
        if (duplicateSummary) {
          publishScan(duplicateSummary);
        }
        if (result.status === "rolled_back") {
          runner.setSectionMessage("duplicates", () =>
            t("settings.actions.summary.duplicateRolledBack")
          );
          return;
        }
        if (result.status === "recovery_required") {
          const recoveryItems = result.results.filter((item) => item.recovery_path);
          const details = recoveryItems
            .map((item) => `#${item.asset_id}: ${item.recovery_path}`)
            .join("; ");
          runner.setSectionMessage("duplicates", () =>
            `${t("settings.actions.summary.duplicateRecoveryRequired", {
              count: recoveryItems.length
            })} ${details}`.trim()
          );
          return;
        }
        const missingSources = result.results.filter(
          (item) => item.status === "source_missing"
        ).length;
        const groups = duplicateSummary?.duplicate_groups ?? latest.groups.length;
        runner.setSectionMessage("duplicates", () => {
          const appliedMessage = t("settings.actions.summary.duplicateApplied", {
            count: changes.length,
            groups
          });
          return missingSources > 0
            ? `${appliedMessage} ${t("lightbox.deleteConfirm.sourceMissing")} (${missingSources})`
            : appliedMessage;
        });
      }
    );
  };
  const onSaveAll = async (changes: DuplicateResolutionChange[]) => {
    if (runner.isOperationLocked) return;
    if (!changes.length) {
      runner.setSectionMessage("duplicates", () =>
        t("settings.actions.summary.noDuplicateChanges")
      );
      return;
    }
    const deleteCount = changes.filter((change) => change.type === "delete").length;
    if (deleteCount > 0) {
      setPendingDuplicateDeleteConfirm({
        changes: changes.map((change) => ({ ...change })),
        deleteCount
      });
      return;
    }
    await runSaveDuplicateChanges(changes);
  };
  const onCancelDuplicateDeleteConfirm = () => {
    if (runner.isOperationLocked) {
      return;
    }
    setPendingDuplicateDeleteConfirm(null);
    runner.setSectionMessage("duplicates", () =>
      t("settings.actions.summary.duplicateApplyCancelled")
    );
  };
  const onConfirmDuplicateDeleteConfirm = async () => {
    if (runner.isOperationLocked) return;
    const pending = pendingDelete;
    if (!pending) {
      return;
    }
    setPendingDuplicateDeleteConfirm(null);
    await runSaveDuplicateChanges(pending.changes);
  };
  const onCloseDuplicateDialog = () => {
    setDuplicateDialogOpen(false);
  };
  return {
    get isOperationLocked() {
      return runner.isOperationLocked;
    },
    get operationState() {
      return runner.duplicateOperationState;
    },
    get duplicateDialogOpen() {
      return duplicateDialogOpen();
    },
    get pendingDuplicateDeleteConfirm() {
      return pendingDuplicateDeleteConfirm();
    },
    get groups() {
      return scanResult().groups;
    },
    get assetCount() {
      return scanResult().duplicate_assets;
    },
    onStart,
    onCloseDuplicateDialog,
    onRescan,
    onSaveAll,
    onCancelDuplicateDeleteConfirm,
    onConfirmDuplicateDeleteConfirm
  };
}
