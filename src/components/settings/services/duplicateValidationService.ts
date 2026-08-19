import { z } from "zod";
import type { DuplicateGroup } from "../../../types";
import type { DuplicateResolutionChange } from "../types";
import i18n from "../../../i18n";

const INVALID_FILE_NAME_CHARACTERS = /[\\/:*?"<>|]/;

const VALIDATION_FILE_NAME_EMPTY = "validation.fileNameCannotBeEmpty";
const VALIDATION_FILE_NAME_INVALID = "validation.fileNameInvalid";
const VALIDATION_FILE_NAME_INVALID_CHARACTERS = "validation.fileNameInvalidCharacters";
const VALIDATION_DUPLICATE_PAYLOAD_INVALID = "validation.duplicatePayloadInvalid";

const duplicateFileNameSchema = z
  .string()
  .trim()
  .min(1, VALIDATION_FILE_NAME_EMPTY)
  .refine((value) => value !== "." && value !== "..", VALIDATION_FILE_NAME_INVALID)
  .refine((value) => !INVALID_FILE_NAME_CHARACTERS.test(value), VALIDATION_FILE_NAME_INVALID_CHARACTERS);

const duplicateResolutionChangeSchema = z.discriminatedUnion("type", [
  z.object({
    assetId: z.number().int().positive(),
    type: z.literal("delete")
  }),
  z.object({
    assetId: z.number().int().positive(),
    type: z.literal("rename"),
    nextFileName: duplicateFileNameSchema
  })
]);

export interface DuplicateChangesValidationResult {
  valid: boolean;
  firstError: string | null;
  errors: string[];
}

function resolveValidationMessage(keyOrText: string): string {
  const translated = i18n.t(keyOrText);
  return translated === keyOrText ? keyOrText : translated;
}

export function getFileNameFromPath(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

function normalizeFileNameForCompare(fileName: string): string {
  return fileName.trim().toLowerCase();
}

export function validateDuplicateResolutionChanges(
  groups: DuplicateGroup[],
  stagedChanges: DuplicateResolutionChange[]
): DuplicateChangesValidationResult {
  const parsedChangesResult = z.array(duplicateResolutionChangeSchema).safeParse(stagedChanges);
  if (!parsedChangesResult.success) {
    const firstIssue = resolveValidationMessage(
      parsedChangesResult.error.issues[0]?.message ?? VALIDATION_DUPLICATE_PAYLOAD_INVALID
    );
    return {
      valid: false,
      firstError: firstIssue,
      errors: [firstIssue]
    };
  }

  const parsedChanges = parsedChangesResult.data;
  const knownAssetIds = new Set<number>();
  for (const group of groups) {
    for (const asset of group.assets) {
      knownAssetIds.add(asset.id);
    }
  }

  const errors: string[] = [];
  for (const change of parsedChanges) {
    if (!knownAssetIds.has(change.assetId)) {
      errors.push(i18n.t("validation.unknownAssetInChanges", { assetId: change.assetId }));
    }
  }

  const stagedByAssetId = new Map<number, DuplicateResolutionChange>(
    parsedChanges.map((change) => [change.assetId, change])
  );
  const stagedByAssetIdRecord: Record<number, DuplicateResolutionChange> = {};
  for (const [assetId, change] of stagedByAssetId) {
    stagedByAssetIdRecord[assetId] = change;
  }

  for (const group of groups) {
    if (!isDuplicateGroupResolved(group, stagedByAssetIdRecord)) {
      const groupAssetIds = new Set(group.assets.map((asset) => asset.id));
      const groupTouched = parsedChanges.some((change) => groupAssetIds.has(change.assetId));
      if (!groupTouched) {
        continue;
      }
      errors.push(
        i18n.t("validation.duplicateGroupStillUnresolved", { fileName: group.file_name })
      );
    }
  }

  return {
    valid: errors.length === 0,
    firstError: errors[0] ?? null,
    errors
  };
}

export function isDuplicateGroupResolved(
  group: DuplicateGroup,
  stagedChangesByAssetId: Record<number, DuplicateResolutionChange>
): boolean {
  const groupTouched = group.assets.some((asset) => stagedChangesByAssetId[asset.id] !== undefined);
  if (!groupTouched) {
    return false;
  }

  const seenFinalNames = new Set<string>();
  for (const asset of group.assets) {
    const staged = stagedChangesByAssetId[asset.id];
    if (staged?.type === "delete") {
      continue;
    }

    const finalName = staged?.type === "rename" ? staged.nextFileName.trim() : getFileNameFromPath(asset.path);
    const finalNameValidation = duplicateFileNameSchema.safeParse(finalName);
    if (!finalNameValidation.success) {
      return false;
    }

    const normalizedFinalName = normalizeFileNameForCompare(finalName);
    if (seenFinalNames.has(normalizedFinalName)) {
      return false;
    }
    seenFinalNames.add(normalizedFinalName);
  }

  return true;
}
