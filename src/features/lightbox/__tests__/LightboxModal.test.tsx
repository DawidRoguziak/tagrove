import { renderHook, fireEvent, screen, waitFor, cleanup } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
import { createLibrary } from "../../../library/createLibrary";
import { createLightbox } from "../createLightbox";
import LightboxModal from "../LightboxModal";
import { UiModal } from "../../../ui/overlays/UiModal";
import { UiLayerProvider } from "../../../ui/overlays/UiLayerProvider";
import { SuggestionClient } from "../../search/suggestions/SuggestionClient";
import { renderSolid } from "../../../test/renderSolid";
import { settle } from "../../../test/settle";
import { asset, summary, deferred, saved } from "./fixtures";
import i18n from "../../../i18n/core";
import type { AssetDetails } from "../../../platform/tauri/contracts/assets";
const api = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetDetails: vi.fn(),
  getAssetQueryPosition: vi.fn(),
  setAssetTags: vi.fn(),
  setAssetFavorite: vi.fn(),
  setAssetMediaGroup: vi.fn(),
  deleteAsset: vi.fn(),
  listTags: vi.fn(),
  toMediaSrc: (path: string) => `asset://${path}`
}));
vi.mock("../../../platform/tauri/assets", () => api);
vi.mock("../../../platform/tauri/tags", () => api);
const tick = () =>
  settle(async () => {
    for (let i = 0; i < 12; i++) await Promise.resolve();
  });
let narrow = false,
  reduced = false;
beforeEach(() => {
  vi.resetAllMocks();
  narrow = reduced = false;
  window.matchMedia = vi.fn((query: string) => ({
    matches: query.includes("767px") ? narrow : query.includes("reduced") ? reduced : false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: () => true
  }));
  api.startAssetQuery.mockResolvedValue({
    status: "ready",
    session_id: 1,
    revision: 1,
    offset: 0,
    total: 3,
    items: [summary(1), summary(2), summary(3)]
  });
  api.getAssetDetails.mockImplementation(async (id: number) => asset(id));
  api.setAssetTags.mockImplementation(async (id: number, tags: string[]) => saved(id, tags));
  api.listTags.mockResolvedValue({ items: ["cat", "dog", "travel"], total: 3 });
  api.deleteAsset.mockResolvedValue({
    removed_assets: 1,
    removed_thumbnails: 0,
    source_status: "deleted",
    revision: 2,
    recovery_path: null
  });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});
async function setup(load = true) {
  const root = renderHook(() => {
    const library = createLibrary();
    return { library, box: createLightbox(library) };
  });
  const { box, library } = root.result;
  await settle(() => library.query.refresh());
  settle(() => box.select(summary(1)));
  if (load) await waitFor(() => expect(box.selected?.path).not.toBeNull());
  const client = new SuggestionClient();
  let openNested = () => {};
  const view = renderSolid(() => {
    const [nested, setNested] = createSignal(false);
    openNested = () => setNested(true);
    return (
      <UiLayerProvider>
        <LightboxModal model={box} client={client} knownTags={["cat", "dog", "travel"]} />
        <UiModal open={nested()} onClose={() => setNested(false)} ariaLabel="Nested">
          <button type="button">Nested action</button>
        </UiModal>
      </UiLayerProvider>
    );
  }, {});
  await settle(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
  return {
    box,
    library,
    view,
    openNested,
    dispose: () => {
      view.unmount();
      root.cleanup();
      client.dispose();
    }
  };
}
const click = async (name: string) => {
  fireEvent.click(screen.getByRole("button", { name }));
  await tick();
};
describe("Solid image lightbox", () => {
  it("renders the reference hierarchy and keeps info collapsed initially", async () => {
    await setup();
    expect(screen.getByRole("dialog")).toHaveAccessibleName("Preview 1.png");
    expect(screen.getByTestId("lightbox-image")).toHaveAttribute("src", "asset:///synthetic/1.png");
    expect(screen.queryByTestId("lightbox-info-panel")).not.toBeInTheDocument();
    await click("Information");
    expect(screen.getByTestId("lightbox-info-panel")).toHaveTextContent("/synthetic/1.png");
  });
  it("navigates without restarting the opening animation or remounting the shell", async () => {
    const { box } = await setup();
    await tick();
    const dialog = screen.getByRole("dialog");
    const overlay = dialog.parentElement;
    const animate = vi.fn();
    Element.prototype.animate = animate;
    settle(box.next);
    await tick();
    expect(screen.getByRole("dialog")).toBe(dialog);
    expect(screen.getByTestId("lightbox-image")).toHaveAttribute("alt", "2.png");
    expect(screen.getByRole("dialog").parentElement).toBe(overlay);
    expect(animate).not.toHaveBeenCalled();
  });
  it("keeps closing content but releases modality immediately", async () => {
    const { box } = await setup();
    const input = screen.getByRole("textbox", { name: "Media group key" });
    fireEvent.input(input, { target: { value: "closing-group" } });
    settle(box.close);
    expect(document.querySelector('[data-modal-presence="closing"]')).toBeInTheDocument();
    expect(screen.getByTestId("lightbox-image")).toHaveAttribute("alt", "1.png");
    expect(input).toHaveValue("closing-group");
    expect(document.body.style.overflow).not.toBe("hidden");
    await waitFor(() => expect(screen.queryByTestId("lightbox-image")).not.toBeInTheDocument());
  });
  it("retains the collapsed panel and save failure throughout exit", async () => {
    const { box } = await setup();
    api.setAssetTags.mockRejectedValueOnce(new Error("save"));
    settle(() => box.saveTags(["draft"]));
    await tick();
    await click("Close asset panel");
    settle(box.close);
    expect(document.getElementById("lightbox-sidebar")).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByTestId("lightbox-tag-panel")).toHaveTextContent("Retry");
    expect(screen.getByTestId("lightbox-tag-list")).toHaveTextContent("draft");
  });
  it("survives close/reopen before the old exit deadline", async () => {
    const { box } = await setup();
    settle(box.close);
    settle(() => box.select(summary(2)));
    await tick();
    await waitFor(() =>
      expect(document.querySelector('[data-modal-presence="open"]')).toBeInTheDocument()
    );
    expect(screen.getByRole("dialog")).toHaveAccessibleName("Preview 2.png");
  });
  it("closes immediately under reduced motion", async () => {
    reduced = true;
    const { box } = await setup();
    expect(document.querySelector('[data-modal-presence="open"]')).toBeInTheDocument();
    settle(box.close);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("keeps collapse and editor drafts across navigation and toggling", async () => {
    const { box } = await setup();
    await click("Close asset panel");
    settle(box.next);
    await tick();
    expect(screen.getByRole("button", { name: "Open asset panel" })).toBeInTheDocument();
    await click("Open asset panel");
    const input = screen.getByRole("combobox", { name: "Add tag" });
    fireEvent.input(input, { target: { value: "draft" } });
    await tick();
    await click("Close asset panel");
    await click("Open asset panel");
    expect(input).toHaveValue("draft");
  });
  it("uses a closed narrow drawer, consumes Escape and restores keyboard focus", async () => {
    narrow = true;
    await setup();
    expect(document.getElementById("lightbox-sidebar")).toHaveAttribute("inert");
    await click("Open asset panel");
    await waitFor(() => expect(document.activeElement?.id).toBe("lightbox-sidebar-close-button"));
    fireEvent.keyDown(window, { key: "Escape" });
    await tick();
    await waitFor(() => expect(document.activeElement?.id).toBe("lightbox-sidebar-trigger-button"));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
  it("blocks navigation under a nested overlay", async () => {
    const { box, openNested } = await setup();
    settle(openNested);
    fireEvent.keyDown(window, { key: "ArrowRight" });
    await tick();
    expect(box.selected?.id).toBe(1);
    fireEvent.keyDown(window, { key: "Escape" });
    await tick();
    expect(screen.getByRole("dialog", { name: "Preview 1.png" })).toBeInTheDocument();
  });
  it("explains an invalid tag, keeps the draft and sends no save", async () => {
    const { box } = await setup();
    const user = userEvent.setup();
    const input = screen.getByRole("combobox", { name: "Add tag" });
    await user.type(input, "two;tags{Enter}");
    await tick();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Tags cannot contain whitespace, commas, semicolons, or control characters."
    );
    expect(input).toHaveValue("two;tags");
    expect(box.tagEditor).toEqual(["cat"]);
    expect(api.setAssetTags).not.toHaveBeenCalled();
    await user.type(input, "{Backspace}");
    await tick();
    expect(screen.queryByTestId("lightbox-tag-invalid")).not.toBeInTheDocument();
  });
  it("keeps input arrows and suggestion Escape inside the editor", async () => {
    const { box } = await setup();
    const user = userEvent.setup();
    const input = screen.getByRole("combobox", { name: "Add tag" });
    await user.type(input, "d");
    await waitFor(() =>
      expect(screen.getByRole("option", { name: /d\s*o\s*g/ })).toBeInTheDocument()
    );
    await user.keyboard("{ArrowRight}{Escape}");
    expect(box.selected?.id).toBe(1);
    expect(input).toHaveValue("d");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
  it("adds suggestions, excludes selected tags and removes chips through canonical saves", async () => {
    const { box } = await setup();
    const user = userEvent.setup();
    const input = screen.getByRole("combobox", { name: "Add tag" });
    await user.type(input, "do");
    await waitFor(() =>
      expect(screen.getByRole("option", { name: /d\s*o\s*g/ })).toBeInTheDocument()
    );
    await user.click(screen.getByRole("option", { name: /d\s*o\s*g/ }));
    await tick();
    expect(box.tagEditor).toEqual(["cat", "dog"]);
    expect(input).toHaveValue("");
    const remove = screen
      .getAllByRole("button")
      .find(
        (button) =>
          button.getAttribute("aria-label")?.includes("cat") &&
          button.getAttribute("aria-label")?.includes("Remove")
      );
    expect(remove).toBeDefined();
    fireEvent.click(remove!);
    await tick();
    expect(box.tagEditor).toEqual(["dog"]);
  });
  it("rejects CSV separators and whitespace without issuing saves", async () => {
    await setup();
    const input = screen.getByRole("combobox", { name: "Add tag" });
    for (const value of ["a b", "a,b", "a;b", "a\u0000b", "cat"]) {
      fireEvent.input(input, { target: { value } });
      await tick();
      fireEvent.keyDown(input, { key: "Enter" });
      await tick();
    }
    expect(api.setAssetTags).not.toHaveBeenCalled();
  });
  it("disables editing during detail loading and offers a details retry", async () => {
    const detail = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValueOnce(detail.promise);
    const { box } = await setup(false);
    expect(screen.getByRole("combobox", { name: "Add tag" })).toBeDisabled();
    expect(screen.getByTestId("lightbox-details-loading")).toBeInTheDocument();
    detail.reject(new Error("read"));
    await tick();
    expect(screen.getByTestId("lightbox-details-error")).toBeInTheDocument();
    expect(box.tagDetailsFailed).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Retry loading details" }));
    await tick();
    expect(screen.getByTestId("lightbox-image")).toBeInTheDocument();
  });
  it("resets media failure on A-B-A and ignores retired image errors", async () => {
    const { box } = await setup();
    const first = screen.getByTestId("lightbox-image");
    fireEvent.error(first);
    await tick();
    expect(screen.getByTestId("lightbox-media-error")).toBeInTheDocument();
    settle(() => box.select(summary(2)));
    await tick();
    settle(() => box.select(summary(1)));
    await tick();
    fireEvent.error(first);
    await tick();
    expect(screen.getByTestId("lightbox-image")).toHaveAttribute("alt", "1.png");
  });
  it("retains the GIF node through details and metadata changes", async () => {
    const { box, library } = await setup();
    const details = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValueOnce(details.promise);
    settle(() => box.select(summary(9, { kind: "gif", preview_path: "/synthetic/9.gif" })));
    const img = screen.getByTestId("lightbox-image");
    expect(document.querySelector('[data-lightbox-kind="gif"]')).toBeInTheDocument();
    details.resolve(
      asset(9, { kind: "gif", path: "/synthetic/9.gif", preview_path: "/synthetic/9.gif" })
    );
    await tick();
    expect(screen.getByTestId("lightbox-image")).toBe(img);
    await settle(() => library.mutations.setFavorite(9, true));
    expect(screen.getByTestId("lightbox-image")).toBe(img);
  });
  it("confirms deletion case-insensitively and prevents duplicate submission", async () => {
    const { box } = await setup();
    const pending = deferred<unknown>();
    api.deleteAsset.mockReturnValueOnce(pending.promise);
    await click("Delete media");
    const confirmButton = screen.getByRole("button", { name: "Confirm" });
    expect(confirmButton).toBeDisabled();
    fireEvent.input(document.getElementById("lightbox-delete-confirm-input")!, {
      target: { value: " yEs " }
    });
    await tick();
    expect(confirmButton).toBeEnabled();
    fireEvent.click(confirmButton);
    fireEvent.click(confirmButton);
    await tick();
    expect(api.deleteAsset).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Close preview" })).toBeDisabled();
    pending.resolve({
      source_status: "deleted",
      removed_assets: 1,
      removed_thumbnails: 0,
      revision: 2,
      recovery_path: null
    });
    await tick();
    expect(box.selected).toBeNull();
  });
  it("matches the confirm word case-insensitively in every locale without localeCompare", async () => {
    // WebKitGTK's localeCompare(..., { sensitivity: "base" }) does not fold case; model that.
    vi.spyOn(String.prototype, "localeCompare").mockImplementation(function (
      this: string,
      other: string
    ) {
      return this === other ? 0 : this < other ? -1 : 1;
    });
    await setup();
    await click("Delete media");
    const input = document.getElementById("lightbox-delete-confirm-input")!;
    const dialog = screen.getByTestId("lightbox-delete-confirm-dialog");
    const confirmButton = () => dialog.querySelectorAll("button")[1]!;
    const typeWord = async (value: string) => {
      fireEvent.input(input, { target: { value } });
      await tick();
    };
    try {
      for (const value of ["YES", "yes", " Yes "]) {
        await typeWord(value);
        expect(confirmButton(), value).toBeEnabled();
      }
      await typeWord("yess");
      expect(confirmButton()).toBeDisabled();
      const cases: Array<[string, string[], string]> = [
        ["pl", ["TAK", "tak", "Tak"], "Yes"],
        ["ru", ["ДА", "да"], "Tak"],
        ["es", ["SÍ", "sí", "Si\u0301"], "si"],
        ["cs", ["ANO", "ano"], "ne"]
      ];
      for (const [language, accepted, rejected] of cases) {
        await settle(() => i18n.changeLanguage(language));
        await tick();
        for (const value of accepted) {
          await typeWord(value);
          expect(confirmButton(), `${language}: ${value}`).toBeEnabled();
        }
        await typeWord(rejected);
        expect(confirmButton(), `${language}: ${rejected}`).toBeDisabled();
      }
    } finally {
      await settle(() => i18n.changeLanguage("en"));
    }
  });
  it("dismisses the confirmation first on Escape with focus outside it", async () => {
    const { box } = await setup();
    await click("Delete media");
    fireEvent.input(document.getElementById("lightbox-delete-confirm-input")!, {
      target: { value: "Yes" }
    });
    await tick();
    const tagInput = screen.getByRole("combobox", { name: "Add tag" });
    tagInput.focus();
    fireEvent.keyDown(tagInput, { key: "Escape" });
    await tick();
    expect(screen.queryByTestId("lightbox-delete-confirm-dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(box.selected?.id).toBe(1);
    await click("Delete media");
    const shell = screen.getByRole("dialog");
    shell.focus();
    fireEvent.keyDown(shell, { key: "Escape" });
    await tick();
    expect(screen.queryByTestId("lightbox-delete-confirm-dialog")).not.toBeInTheDocument();
    expect(box.selected?.id).toBe(1);
    fireEvent.keyDown(window, { key: "Escape" });
    await tick();
    expect(box.selected).toBeNull();
  });
  it("ignores Escape outside the confirmation while a delete is pending", async () => {
    const { box } = await setup();
    const pending = deferred<unknown>();
    api.deleteAsset.mockReturnValueOnce(pending.promise);
    await click("Delete media");
    fireEvent.input(document.getElementById("lightbox-delete-confirm-input")!, {
      target: { value: "Yes" }
    });
    await tick();
    await click("Confirm");
    fireEvent.keyDown(window, { key: "Escape" });
    await tick();
    expect(screen.getByTestId("lightbox-delete-confirm-dialog")).toBeInTheDocument();
    expect(box.selected?.id).toBe(1);
    pending.resolve({
      source_status: "deleted",
      removed_assets: 1,
      removed_thumbnails: 0,
      revision: 2,
      recovery_path: null
    });
    await tick();
    expect(box.selected).toBeNull();
  });
  it("keeps the confirmation, typed word and Information open across tag and favorite writes", async () => {
    api.setAssetFavorite.mockResolvedValue(undefined);
    const { box } = await setup();
    await click("Information");
    expect(screen.getByRole("button", { name: "Information" })).toHaveAttribute(
      "aria-expanded",
      "true"
    );
    await click("Delete media");
    const input = document.getElementById("lightbox-delete-confirm-input") as HTMLInputElement;
    fireEvent.input(input, { target: { value: "Yes" } });
    await tick();
    const before = box.selected;
    await waitFor(() => expect(document.activeElement).toBe(input));
    const tagInput = screen.getByRole("combobox", { name: "Add tag" });
    tagInput.focus();
    fireEvent.input(tagInput, { target: { value: "dog" } });
    await tick();
    fireEvent.keyDown(tagInput, { key: "Enter" });
    await tick();
    await waitFor(() => expect(box.tagEditor).toEqual(["cat", "dog"]));
    await settle(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
    await settle(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
    // The republished selection must not pull focus back into the delete-confirm input.
    expect(document.activeElement).toBe(tagInput);
    expect(box.selected).not.toBe(before);
    expect(box.selected?.id).toBe(1);
    expect(screen.getByTestId("lightbox-delete-confirm-dialog")).toBeInTheDocument();
    expect(input).toHaveValue("Yes");
    expect(screen.getByRole("button", { name: "Confirm" })).toBeEnabled();
    await settle(() => box.toggleFavorite());
    await tick();
    expect(api.setAssetFavorite).toHaveBeenCalledWith(1, true);
    expect(box.selected?.is_favorite).toBe(true);
    expect(screen.getByTestId("lightbox-delete-confirm-dialog")).toBeInTheDocument();
    expect(input).toHaveValue("Yes");
    await click("Confirm");
    expect(api.deleteAsset).toHaveBeenCalledTimes(1);
    expect(box.selected).toBeNull();
  });
  it("still resets the confirmation and Information when the selected ID changes", async () => {
    const { box } = await setup();
    await click("Information");
    await click("Delete media");
    expect(screen.getByTestId("lightbox-delete-confirm-dialog")).toBeInTheDocument();
    settle(() => box.select(summary(2)));
    await tick();
    expect(box.selected?.id).toBe(2);
    expect(screen.queryByTestId("lightbox-delete-confirm-dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Information" })).toHaveAttribute(
      "aria-expanded",
      "false"
    );
  });
  it("keeps failed deletion in its confirmation and restores focus after Escape", async () => {
    api.deleteAsset.mockRejectedValue(new Error("source retained"));
    await setup();
    await click("Delete media");
    const input = document.getElementById("lightbox-delete-confirm-input")!;
    fireEvent.input(input, { target: { value: "Yes" } });
    await tick();
    await click("Confirm");
    expect(screen.getByRole("alert")).toHaveTextContent("source retained");
    fireEvent.keyDown(input, { key: "Escape" });
    await tick();
    expect(screen.queryByTestId("lightbox-delete-confirm-dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(document.activeElement?.id).toBe("lightbox-delete-button");
  });
  it("shows a busy-library delete rejection localized and keeps every exit usable", async () => {
    // The backend fails fast while a scan holds the scan lock (ADR 0022).
    api.deleteAsset.mockRejectedValue("i18n:common.libraryBusy");
    const { box } = await setup();
    await click("Delete media");
    const input = document.getElementById("lightbox-delete-confirm-input")!;
    fireEvent.input(input, { target: { value: "Yes" } });
    await tick();
    await click("Confirm");
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(
      "The file could not be deleted. Its library metadata was preserved. A scan or another library operation is running. Try again when it finishes."
    );
    expect(alert).not.toHaveTextContent("i18n:");
    expect(box.selected?.id).toBe(1);
    expect(input).toBeEnabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Confirm" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Close preview" })).toBeEnabled();
    await click("Cancel");
    expect(screen.queryByTestId("lightbox-delete-confirm-dialog")).not.toBeInTheDocument();
    fireEvent.keyDown(window, { key: "ArrowRight" });
    await tick();
    expect(box.selected?.id).toBe(2);
    await click("Close preview");
    expect(box.selected).toBeNull();
  });
  it("copies trimmed group names and rejects late clipboard results", async () => {
    const write = deferred<void>();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn(() => write.promise) }
    });
    const { box } = await setup();
    settle(() => box.setMediaGroupKeyEditor("  group-one  "));
    await click("Copy group name");
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("group-one");
    settle(box.next);
    await tick();
    write.resolve();
    await tick();
    expect(screen.queryByRole("button", { name: "Group name copied" })).not.toBeInTheDocument();
  });
  it("ignores shortcuts during deletion confirmation and on logical close", async () => {
    const { box } = await setup();
    await click("Delete media");
    fireEvent.keyDown(window, { key: "ArrowRight" });
    await tick();
    expect(box.selected?.id).toBe(1);
    settle(box.close);
    fireEvent.keyDown(window, { key: "ArrowRight" });
    await tick();
    expect(box.selected).toBeNull();
  });
});
