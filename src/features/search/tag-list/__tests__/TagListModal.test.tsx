import { renderSolid } from "../../../../test/renderSolid";
import { settle as act } from "../../../../test/settle";
import { flush } from "solid-js";
import { cleanup, fireEvent, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TagListPage } from "../../../../platform/tauri/contracts/tags";
import * as tagListSelection from "../tagListSelectionService";
const apiMocks = vi.hoisted(() => ({
  listTags: vi.fn()
}));

vi.mock("../../../../platform/tauri/tags", async () => {
  return {
    listTags: apiMocks.listTags
  };
});

function createTagListPage(items: string[] = [], total = items.length): TagListPage {
  return { items, total };
}

interface TagListModalProps {
  open: boolean;
  knownTags?: string[];
  onClose: () => void;
  onApplySearch?: (filterInput: string) => Promise<void> | void;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function flushMicrotasks() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

async function flushSingleClick() {
  await act(async () => {
    vi.advanceTimersByTime(221);
    await Promise.resolve();
  });
}

describe("TagListModal", () => {
  let TagListModal: typeof import("../TagListModal").TagListModal;

  beforeEach(async () => {
    apiMocks.listTags.mockReset();
    apiMocks.listTags.mockResolvedValue(createTagListPage());
    ({ TagListModal } = await import("../TagListModal"));
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("does not prepare the tag vocabulary while closed, including after updates", async () => {
    apiMocks.listTags.mockRejectedValue(new Error("offline"));
    const dedupe = vi.spyOn(tagListSelection, "dedupeKnownTags");
    const props = { open: false, knownTags: ["beta", "alpha"], onClose: vi.fn() };
    const { rerender } = renderSolid(TagListModal, { ...props });
    rerender({ ...props, knownTags: ["gamma", "delta"] });

    expect(dedupe).not.toHaveBeenCalled();
    expect(apiMocks.listTags).not.toHaveBeenCalled();

    rerender({ ...props, open: true });
    expect(dedupe).toHaveBeenCalledWith(props.knownTags);
    expect(
      await screen.findByRole("button", { name: "alpha, state: inactive" })
    ).toBeInTheDocument();
    dedupe.mockClear();
    rerender({ ...props });
    const updatedTags = ["zeta", "epsilon"];
    rerender({ ...props, knownTags: updatedTags });
    expect(dedupe).not.toHaveBeenCalled();
    rerender({ ...props, knownTags: updatedTags, open: true });
    expect(dedupe).toHaveBeenCalledWith(updatedTags);
    expect(
      await screen.findByRole("button", { name: "epsilon, state: inactive" })
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "alpha, state: inactive" })
    ).not.toBeInTheDocument();
  });

  it("opens lazily, requests the first page and focuses the filter input", async () => {
    const initialProps: TagListModalProps = {
      open: false,
      onClose: vi.fn(),
      onApplySearch: vi.fn()
    };
    const { rerender } = renderSolid(TagListModal, { ...initialProps });

    expect(screen.queryByTestId("tag-list-modal")).not.toBeInTheDocument();

    rerender({ ...initialProps, open: true });

    const filterInput = await screen.findByLabelText("Filter tags");

    await waitFor(() => {
      expect(apiMocks.listTags).toHaveBeenCalledWith({
        query: "",
        offset: 0,
        limit: 100
      });
    });
    await waitFor(() => {
      expect(filterInput).toHaveFocus();
    });
  });

  it("falls back to deduplicated known tags and filters them locally after request failures", async () => {
    apiMocks.listTags.mockRejectedValue(new Error("offline"));

    renderSolid(TagListModal, {
      open: true,
      knownTags: [" Zebra ", "alpha", "Alpha", "beta"],
      onClose: vi.fn(),
      onApplySearch: vi.fn()
    });

    expect(
      await screen.findByRole("button", { name: "alpha, state: inactive" })
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "beta, state: inactive" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Zebra, state: inactive" })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Alpha, state: inactive" })
    ).not.toBeInTheDocument();

    fireEvent.input(screen.getByLabelText("Filter tags"), { target: { value: "alp" } });
    flush();

    await waitFor(() => {
      expect(apiMocks.listTags).toHaveBeenLastCalledWith({
        query: "alp",
        offset: 0,
        limit: 100
      });
    });
    expect(
      await screen.findByRole("button", { name: "alpha, state: inactive" })
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "beta, state: inactive" })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Zebra, state: inactive" })
    ).not.toBeInTheDocument();
  });

  it("tracks single and double click selections and applies a sorted filter string", async () => {
    vi.useFakeTimers();
    apiMocks.listTags.mockResolvedValueOnce(createTagListPage(["banana", "Apple", "cherry"]));
    const onApplySearch = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    renderSolid(TagListModal, { open: true, onClose: onClose, onApplySearch: onApplySearch });
    await flushMicrotasks();

    fireEvent.dblClick(screen.getByRole("button", { name: "cherry, state: inactive" }));
    fireEvent.click(screen.getByRole("button", { name: "banana, state: inactive" }));
    flush();
    await flushSingleClick();
    fireEvent.click(screen.getByRole("button", { name: "Apple, state: inactive" }));
    flush();
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    flush();
    await flushMicrotasks();

    expect(onApplySearch).toHaveBeenCalledWith("Apple banana -cherry");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("uses Shift+Enter as the keyboard equivalent of tag exclusion", async () => {
    apiMocks.listTags.mockResolvedValueOnce(createTagListPage(["cherry"]));
    const onApplySearch = vi.fn().mockResolvedValue(undefined);
    renderSolid(TagListModal, { open: true, onClose: vi.fn(), onApplySearch: onApplySearch });

    const tag = await screen.findByRole("button", { name: "cherry, state: inactive" });
    tag.focus();
    fireEvent.keyDown(tag, { key: "Enter", shiftKey: true });
    flush();
    expect(screen.getByRole("button", { name: "cherry, state: excluded" })).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    flush();

    await waitFor(() => expect(onApplySearch).toHaveBeenCalledWith("-cherry"));
  });

  for (const name of ["constructor", "__proto__"]) {
    it.each(["single", "double", "keyboard"])(`applies literal ${name} through %s selection`, async (action) => {
      apiMocks.listTags.mockResolvedValue(createTagListPage([name]));
      const onApplySearch = vi.fn().mockResolvedValue(undefined);
      renderSolid(TagListModal, { open: true, onClose: vi.fn(), onApplySearch });
      const tag = await screen.findByRole("button", { name: `${name}, state: inactive` });
      if (action === "single") fireEvent.click(tag);
      else if (action === "double") fireEvent.dblClick(tag);
      else {
        tag.focus();
        fireEvent.keyDown(tag, { key: "Enter", shiftKey: true });
      }
      flush();
      fireEvent.click(screen.getByRole("button", { name: "Apply" }));
      flush();
      await waitFor(() => expect(onApplySearch).toHaveBeenCalledExactlyOnceWith(
        `${action === "single" ? "" : "-"}${name}`
      ));
    });
  }

  it("keeps the modal open when applying the search fails", async () => {
    vi.useFakeTimers();
    apiMocks.listTags.mockResolvedValueOnce(createTagListPage(["Alpha"]));
    const onApplySearch = vi.fn().mockRejectedValue(new Error("save failed"));
    const onClose = vi.fn();

    renderSolid(TagListModal, { open: true, onClose: onClose, onApplySearch: onApplySearch });
    await flushMicrotasks();

    fireEvent.click(screen.getByRole("button", { name: "Alpha, state: inactive" }));
    flush();
    await flushSingleClick();
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    flush();
    await flushMicrotasks();

    expect(onApplySearch).toHaveBeenCalledWith("Alpha");
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByTestId("tag-list-modal")).toBeInTheDocument();
  });

  it("loads the next page on scroll and merges it without duplicates", async () => {
    const firstPage = deferred<TagListPage>();
    apiMocks.listTags
      .mockReturnValueOnce(firstPage.promise)
      .mockResolvedValueOnce(createTagListPage(["Beta", "Gamma"], 4));

    renderSolid(TagListModal, { open: true, onClose: vi.fn(), onApplySearch: vi.fn() });

    const list = screen.getByLabelText("Tag list results");
    Object.defineProperty(list, "clientHeight", { configurable: true, value: 100 });
    Object.defineProperty(list, "scrollHeight", { configurable: true, value: 400 });
    Object.defineProperty(list, "scrollTop", { configurable: true, value: 0, writable: true });

    await act(async () => {
      firstPage.resolve(createTagListPage(["Alpha", "Beta"], 4));
      await Promise.resolve();
    });

    expect(
      await screen.findByRole("button", { name: "Alpha, state: inactive" })
    ).toBeInTheDocument();

    Object.defineProperty(list, "scrollTop", { configurable: true, value: 260, writable: true });
    fireEvent.scroll(list);
    flush();

    await waitFor(() => {
      expect(apiMocks.listTags).toHaveBeenNthCalledWith(2, {
        query: "",
        offset: 2,
        limit: 100
      });
    });
    expect(
      await screen.findByRole("button", { name: "Gamma, state: inactive" })
    ).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Beta, state: inactive" })).toHaveLength(1);
  });
});
