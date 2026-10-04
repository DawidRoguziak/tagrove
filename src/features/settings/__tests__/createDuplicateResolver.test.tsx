import { renderHook } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { LanguageProvider } from "../../../i18n/solid";
import { settle } from "../../../test/settle";
import { createDuplicateResolver } from "../createDuplicateResolver";
import type { DuplicateGroup } from "../../../types";
const groups: DuplicateGroup[] = [
  {
    file_name: "same.jpg",
    assets: [
      { id: 1, path: "/synthetic/a/same.jpg", record_version: 1, size_bytes: 10 },
      { id: 7, path: "/synthetic/b/same.jpg", record_version: 1, size_bytes: 10 }
    ]
  }
];
function setup() {
  const save = vi.fn(async () => {});
  const view = renderHook(
    () => {
      const [open, setOpen] = createSignal(true),
        [items, setItems] = createSignal(groups);
      const model = createDuplicateResolver({
        get open() {
          return open();
        },
        get groups() {
          return items();
        },
        onSaveAll: save
      });
      return { model, setOpen, setItems };
    },
    { wrapper: LanguageProvider }
  );
  return { ...view.result, save };
}
describe("duplicate form ownership and same-turn commands", () => {
  it("updates a staged rename, preserving the input draft and trimming the command", async () => {
    const { model, save } = setup();
    settle(() => {
      model.rename(1, "first.jpg");
      model.queueRename(1);
      model.rename(1, " next-name.png ");
    });
    expect(model.renameValues[1]).toBe(" next-name.png ");
    expect(model.stagedChanges).toEqual([
      { assetId: 1, type: "rename", nextFileName: "next-name.png" }
    ]);
    await model.save();
    expect(save).toHaveBeenCalledWith(model.stagedChanges);
  });
  it("queues by ID, replaces a rename with delete, unmarks and clears without stale reads", () => {
    const { model } = setup();
    settle(() => {
      model.rename(7, " queued.jpg ");
      model.queueRename(7);
    });
    expect(model.stagedChanges).toEqual([
      { assetId: 7, type: "rename", nextFileName: "queued.jpg" }
    ]);
    settle(() => model.toggleDelete(7));
    expect(model.stagedChanges).toEqual([{ assetId: 7, type: "delete" }]);
    settle(() => {
      model.toggleDelete(7);
      model.toggleDelete(7);
      model.toggleDelete(7);
    });
    expect(model.stagedChanges).toEqual([]);
    settle(() => {
      model.queueRename(7);
      model.clear(7);
    });
    expect(model.stagedChanges).toEqual([]);
  });
  it("generates a draft with its extension without staging, and resets on reopen or scan replacement", () => {
    const { model, setOpen, setItems } = setup();
    settle(() => model.generateUuid(1, "same.jpg"));
    expect(model.renameValues[1]).toMatch(/^[\w-]+\.jpg$/);
    expect(model.stagedChanges).toEqual([]);
    settle(() => model.queueRename(1));
    expect(model.validationResult.valid).toBe(true);
    settle(() => setOpen(false));
    settle(() => setOpen(true));
    expect(model.renameValues[1]).toBe("same.jpg");
    expect(model.stagedChanges).toEqual([]);
    settle(() => {
      model.rename(1, "renamed.jpg");
      model.queueRename(1);
    });
    settle(() => setItems([]));
    expect(model.stagedChanges).toEqual([]);
    expect(model.renameValues).toEqual({});
  });
});
