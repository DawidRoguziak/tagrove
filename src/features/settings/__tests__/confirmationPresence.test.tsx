import { createSignal } from "solid-js";
import { expect, it } from "vitest";
import { renderSettings } from "./renderSettings";
import { settle } from "../../../test/settle";
import { DuplicateDeleteConfirmDialog } from "../DuplicateDeleteConfirmDialog";
import { RemoveScanRootConfirmDialog } from "../RemoveScanRootConfirmDialog";

it("retains the duplicate counts in the exiting dialog after confirmation clears them", () => {
  const [state, setState] = createSignal({ open: true, count: 2, deleteCount: 1 });
  renderSettings(() => (
    <DuplicateDeleteConfirmDialog
      {...state()}
      isOperationLocked={false}
      onCancel={() => {}}
      onConfirm={() => {}}
    />
  ));
  const heading = document.getElementById("duplicate-delete-confirm-heading");
  expect(heading?.parentElement).toHaveTextContent("Apply 2 queued changes?");
  settle(() => setState({ open: false, count: 0, deleteCount: 0 }));
  expect(heading?.closest("[data-modal-presence]")).toHaveAttribute(
    "data-modal-presence",
    "closing"
  );
  expect(heading?.parentElement).toHaveTextContent("Apply 2 queued changes?");
  expect(heading?.parentElement).toHaveTextContent("permanently delete 1 files");
});
it("retains the root path through the remove confirmation exit", () => {
  const [path, setPath] = createSignal<string | null>("/synthetic/media");
  renderSettings(() => (
    <RemoveScanRootConfirmDialog
      path={path()}
      isOperationLocked={false}
      onCancel={() => {}}
      onConfirm={() => {}}
    />
  ));
  const heading = document.getElementById("remove-scan-root-confirm-heading");
  settle(() => setPath(null));
  expect(heading?.closest("[data-modal-presence]")).toHaveAttribute(
    "data-modal-presence",
    "closing"
  );
  expect(heading?.parentElement).toHaveTextContent("/synthetic/media");
});
