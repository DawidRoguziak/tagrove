import { createSignal } from "solid-js";
import { settle } from "../../../test/settle";
import { renderSettings as render } from "./renderSettings";
import { ThumbnailStore } from "../../../library/thumbnails/thumbnailStore";
import { cleanup, fireEvent, screen } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DuplicateGroup } from "../../../platform/tauri/contracts/maintenance";
import { createSectionOperationState } from "../operationStateService";
import { DuplicateResolverDialog } from "../DuplicateResolverDialog";
function makeGroups(): DuplicateGroup[] {
  return [
    {
      file_name: "same.jpg",
      assets: [
        {
          id: 1,
          path: "C:/media/a/same.jpg",
          record_version: 1,
          size_bytes: 10
        },
        {
          id: 2,
          path: "C:/media/b/same.jpg",
          record_version: 1,
          size_bytes: 20
        }
      ]
    }
  ];
}
describe("DuplicateResolverDialog", () => {
  afterEach(() => {
    cleanup();
  });
  it("does not render when closed", () => {
    render(() => (
      <DuplicateResolverDialog
        thumbnailStore={new ThumbnailStore()}
        onQueueThumbnailsByIds={() => {}}
        open={false}
        isOperationLocked={false}
        operationState={createSectionOperationState("duplicates")}
        groups={makeGroups()}
        onClose={() => {}}
        onRescan={() => {}}
        onSaveAll={vi.fn(async () => {})}
      />
    ));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("queues rename and submits all staged changes", async () => {
    const onSaveAll = vi.fn(async () => {});
    render(() => (
      <DuplicateResolverDialog
        thumbnailStore={new ThumbnailStore()}
        onQueueThumbnailsByIds={() => {}}
        open
        isOperationLocked={false}
        operationState={createSectionOperationState("duplicates")}
        groups={makeGroups()}
        onClose={() => {}}
        onRescan={() => {}}
        onSaveAll={onSaveAll}
      />
    ));
    const nameInputs = screen.getAllByRole("textbox");
    await userEvent.clear(nameInputs[0]);
    await userEvent.type(nameInputs[0], "renamed.jpg");
    await userEvent.click(screen.getAllByRole("button", { name: "Queue rename" })[0]);
    const saveButton = screen.getByRole("button", { name: "Save all" });
    expect(saveButton).toBeEnabled();
    await userEvent.click(saveButton);
    expect(onSaveAll).toHaveBeenCalledTimes(1);
    expect(onSaveAll).toHaveBeenCalledWith([
      {
        assetId: 1,
        type: "rename",
        nextFileName: "renamed.jpg"
      }
    ]);
  });
  it("blocks save when queued changes still keep duplicate names", async () => {
    render(() => (
      <DuplicateResolverDialog
        thumbnailStore={new ThumbnailStore()}
        onQueueThumbnailsByIds={() => {}}
        open
        isOperationLocked={false}
        operationState={createSectionOperationState("duplicates")}
        groups={makeGroups()}
        onClose={() => {}}
        onRescan={() => {}}
        onSaveAll={vi.fn(async () => {})}
      />
    ));
    await userEvent.click(screen.getAllByRole("button", { name: "Queue rename" })[0]);
    expect(screen.getByText(/still leave duplicate file names/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save all" })).toBeDisabled();
  });
  it("closes on backdrop click only when unlocked", () => {
    const onClose = vi.fn();
    const { rerender } = render(() => (
      <DuplicateResolverDialog
        thumbnailStore={new ThumbnailStore()}
        onQueueThumbnailsByIds={() => {}}
        open
        isOperationLocked={false}
        operationState={createSectionOperationState("duplicates")}
        groups={makeGroups()}
        onClose={onClose}
        onRescan={() => {}}
        onSaveAll={vi.fn(async () => {})}
      />
    ));
    fireEvent.mouseDown(screen.getByRole("dialog").parentElement as HTMLElement);
    expect(onClose).toHaveBeenCalledTimes(1);
    rerender(() => (
      <DuplicateResolverDialog
        thumbnailStore={new ThumbnailStore()}
        onQueueThumbnailsByIds={() => {}}
        open
        isOperationLocked
        operationState={createSectionOperationState("duplicates")}
        groups={makeGroups()}
        onClose={onClose}
        onRescan={() => {}}
        onSaveAll={vi.fn(async () => {})}
      />
    ));
    fireEvent.mouseDown(screen.getByRole("dialog").parentElement as HTMLElement);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
  it("queues delete action and allows clearing it", async () => {
    render(() => (
      <DuplicateResolverDialog
        thumbnailStore={new ThumbnailStore()}
        onQueueThumbnailsByIds={() => {}}
        open
        isOperationLocked={false}
        operationState={createSectionOperationState("duplicates")}
        groups={makeGroups()}
        onClose={() => {}}
        onRescan={() => {}}
        onSaveAll={vi.fn(async () => {})}
      />
    ));
    await userEvent.click(screen.getAllByRole("button", { name: "Queue delete" })[0]);
    expect(screen.getByText("Pending: delete file")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Unmark delete" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Clear action" }));
    expect(screen.queryByText("Pending: delete file")).not.toBeInTheDocument();
  });
});

it("updates the mounted virtual list when a rescan removes every group", () => {
  const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(720);
  const width = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  const queued = vi.fn();
  const [groups, setGroups] = createSignal(makeGroups());
  const store = new ThumbnailStore();
  render(() => (
    <DuplicateResolverDialog
      open
      isOperationLocked={false}
      operationState={createSectionOperationState("duplicates")}
      groups={groups()}
      thumbnailStore={store}
      onQueueThumbnailsByIds={queued}
      onClose={() => {}}
      onRescan={() => {}}
      onSaveAll={vi.fn(async () => {})}
    />
  ));
  expect(screen.getAllByRole("textbox")).toHaveLength(2);
  expect(queued).toHaveBeenCalledWith([1, 2]);
  settle(() => setGroups([]));
  expect(screen.queryAllByRole("textbox")).toHaveLength(0);
  expect(screen.getByText("No duplicate groups found.")).toBeInTheDocument();
  height.mockRestore();
  width.mockRestore();
});
