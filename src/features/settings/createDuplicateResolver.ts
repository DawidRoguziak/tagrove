import { createEffect, createMemo, createSignal } from "solid-js";
import {
  getFileNameFromPath,
  validateDuplicateResolutionChanges
} from "../../components/settings/services/duplicateValidationService";
import { useLanguage } from "../../i18n/solid";
import type { DuplicateGroup } from "../../types";
import type { DuplicateResolutionChange } from "../../components/settings/types";

export function createDuplicateResolver(options: {
  open: boolean;
  groups: DuplicateGroup[];
  onSaveAll: (changes: DuplicateResolutionChange[]) => Promise<void>;
}) {
  const language = useLanguage();
  const [renameValues, publishNames] = createSignal<Record<number, string>>({});
  const [staged, publishStaged] = createSignal<Record<number, DuplicateResolutionChange>>({});
  let names: Record<number, string> = {};
  let changes: Record<number, DuplicateResolutionChange> = {};
  function resetForm() {
    names = Object.fromEntries(
      options.groups.flatMap((group) =>
        group.assets.map((asset) => [asset.id, getFileNameFromPath(asset.path)])
      )
    );
    changes = {};
    publishNames(names);
    publishStaged(changes);
  }
  createEffect(
    () => ({ open: options.open, groups: options.groups }),
    ({ open }) => {
      if (open) resetForm();
    }
  );
  const stagedChanges = createMemo(() => Object.values(staged()));
  const validationResult = createMemo(() => {
    language.language();
    return validateDuplicateResolutionChanges(options.groups, stagedChanges());
  });
  function stage(change: DuplicateResolutionChange) {
    changes = { ...changes, [change.assetId]: change };
    publishStaged(changes);
  }
  return {
    get renameValues() {
      return renameValues();
    },
    get stagedChangesByAssetId() {
      return staged();
    },
    get stagedChanges() {
      return stagedChanges();
    },
    get validationResult() {
      return validationResult();
    },
    get duplicateAssetCount() {
      return options.groups.reduce((count, group) => count + group.assets.length, 0);
    },
    resetForm,
    rename(assetId: number, value: string) {
      names = { ...names, [assetId]: value };
      publishNames(names);
      if (changes[assetId]?.type === "rename")
        stage({ assetId, type: "rename", nextFileName: value.trim() });
    },
    generateUuid(assetId: number, currentName: string) {
      const dot = currentName.lastIndexOf(".");
      const extension = dot > 0 ? currentName.slice(dot) : "";
      const uuid =
        window.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
      names = { ...names, [assetId]: `${uuid}${extension}` };
      publishNames(names);
    },
    queueRename(assetId: number) {
      const nextFileName = names[assetId]?.trim();
      if (nextFileName) stage({ assetId, type: "rename", nextFileName });
    },
    toggleDelete(assetId: number) {
      if (changes[assetId]?.type === "delete") {
        const next = { ...changes };
        delete next[assetId];
        changes = next;
        publishStaged(next);
      } else stage({ assetId, type: "delete" });
    },
    clear(assetId: number) {
      const next = { ...changes };
      delete next[assetId];
      changes = next;
      publishStaged(next);
    },
    save: () => options.onSaveAll(Object.values(changes))
  };
}
