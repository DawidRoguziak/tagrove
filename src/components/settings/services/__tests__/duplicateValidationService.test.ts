import { describe, expect, it } from "vitest";
import type { DuplicateGroup } from "../../../../types";
import type { DuplicateResolutionChange } from "../../types";
import {
  getFileNameFromPath,
  isDuplicateGroupResolved,
  validateDuplicateResolutionChanges
} from "../duplicateValidationService";

function makeGroup(fileName: string, ids: number[]): DuplicateGroup {
  return {
    file_name: fileName,
    assets: ids.map((id, index) => ({
      id,
      path: `C:/media/folder-${index + 1}/${fileName}`
    }))
  };
}

describe("duplicateValidationService", () => {
  it("accepts three changes for a 4-item duplicate block", () => {
    const groups = [makeGroup("same.jpg", [1, 2, 3, 4])];
    const changes: DuplicateResolutionChange[] = [
      { assetId: 1, type: "rename", nextFileName: "same-1.jpg" },
      { assetId: 2, type: "rename", nextFileName: "same-2.jpg" },
      { assetId: 3, type: "rename", nextFileName: "same-3.jpg" }
    ];

    const result = validateDuplicateResolutionChanges(groups, changes);
    expect(result.valid).toBe(true);
    expect(result.firstError).toBeNull();
  });

  it("rejects block when queued changes still leave duplicate names", () => {
    const groups = [makeGroup("same.jpg", [1, 2, 3, 4])];
    const changes: DuplicateResolutionChange[] = [
      { assetId: 1, type: "rename", nextFileName: "same-1.jpg" },
      { assetId: 2, type: "rename", nextFileName: "same-2.jpg" }
    ];

    const result = validateDuplicateResolutionChanges(groups, changes);
    expect(result.valid).toBe(false);
    expect(result.firstError).toContain("still leave duplicate file names");
  });

  it("accepts delete operations when they leave one item in block", () => {
    const groups = [makeGroup("same.jpg", [1, 2, 3, 4])];
    const changes: DuplicateResolutionChange[] = [
      { assetId: 1, type: "delete" },
      { assetId: 2, type: "delete" },
      { assetId: 3, type: "delete" }
    ];

    const result = validateDuplicateResolutionChanges(groups, changes);
    expect(result.valid).toBe(true);
  });

  it("validates rename file name with zod schema", () => {
    const groups = [makeGroup("same.jpg", [1, 2])];
    const changes: DuplicateResolutionChange[] = [
      { assetId: 1, type: "rename", nextFileName: "bad/name.jpg" }
    ];

    const result = validateDuplicateResolutionChanges(groups, changes);
    expect(result.valid).toBe(false);
    expect(result.firstError).toContain("invalid characters");
  });

  it("rejects staged change for unknown asset id", () => {
    const groups = [makeGroup("same.jpg", [1, 2])];
    const changes: DuplicateResolutionChange[] = [
      { assetId: 99, type: "rename", nextFileName: "other.jpg" }
    ];

    const result = validateDuplicateResolutionChanges(groups, changes);
    expect(result.valid).toBe(false);
    expect(result.firstError).toContain("Unknown asset in queued changes: 99");
  });

  it("treats file name collisions as case-insensitive", () => {
    const group = makeGroup("same.jpg", [1, 2]);
    const resolved = isDuplicateGroupResolved(group, {
      1: { assetId: 1, type: "rename", nextFileName: "SAME.JPG" }
    });

    expect(resolved).toBe(false);
  });

  it("does not fail untouched duplicate groups", () => {
    const groups = [makeGroup("same.jpg", [1, 2]), makeGroup("other.jpg", [3, 4])];
    const changes: DuplicateResolutionChange[] = [
      { assetId: 1, type: "rename", nextFileName: "same-a.jpg" }
    ];

    const result = validateDuplicateResolutionChanges(groups, changes);
    expect(result.valid).toBe(true);
  });

  it("rejects special dot names as invalid file names", () => {
    const groups = [makeGroup("same.jpg", [1, 2])];
    const changes: DuplicateResolutionChange[] = [{ assetId: 1, type: "rename", nextFileName: "." }];

    const result = validateDuplicateResolutionChanges(groups, changes);
    expect(result.valid).toBe(false);
    expect(result.firstError).toContain("invalid");
  });

  it("extracts file name from unix and windows path", () => {
    expect(getFileNameFromPath("C:/media/folder/name.jpg")).toBe("name.jpg");
    expect(getFileNameFromPath("C:\\media\\folder\\name.jpg")).toBe("name.jpg");
  });
});
