import { cleanup, fireEvent, screen, waitFor } from "@solidjs/testing-library";
import { flush } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { renderSolid } from "../../../test/renderSolid";
import { SearchTagatorWrapper } from "../SearchTagatorWrapper";
import { SuggestionClient } from "../../../components/search/worker/SuggestionClient";
vi.mock("../../../platform/tauri/tags", () => ({
  listTags: vi.fn().mockResolvedValue({ items: [], total: 0 })
}));
afterEach(cleanup);
function mount() {
  const props = {
    client: new SuggestionClient(),
    filterInput: "draft",
    onFilterChange: vi.fn(),
    knownTags: [],
    mediaKind: "all" as const,
    onMediaKindChange: vi.fn(),
    onSearchSubmit: vi.fn(),
    onApplyTagListSearch: vi.fn().mockResolvedValue(undefined),
    onClearAll: vi.fn(),
    favoritesOnly: false,
    onFavoritesOnlyChange: vi.fn()
  };
  return { ...renderSolid(SearchTagatorWrapper, props), props };
}
it("focuses search with Ctrl/Meta+K, ignores modified/composing shortcuts, inert content and modal dialogs", () => {
  const view = mount(),
    input = screen.getByRole("combobox");
  for (const modifier of [{ ctrlKey: true }, { metaKey: true }]) {
    fireEvent.keyDown(window, { key: "k", ...modifier });
    flush();
    expect(input).toHaveFocus();
    (input as HTMLInputElement).blur();
  }
  for (const extra of [{ shiftKey: true }, { altKey: true }, { isComposing: true }]) {
    fireEvent.keyDown(window, { key: "k", ctrlKey: true, ...extra });
    flush();
    expect(input).not.toHaveFocus();
  }
  view.container.setAttribute("inert", "");
  fireEvent.keyDown(window, { key: "k", ctrlKey: true });
  flush();
  expect(input).not.toHaveFocus();
  view.container.removeAttribute("inert");
  fireEvent.click(screen.getByRole("button", { name: "Open tag list" }));
  flush();
  fireEvent.keyDown(window, { key: "k", ctrlKey: true });
  flush();
  expect(input).not.toHaveFocus();
});
it("submits each kind/favorite command once, shows validation and returns focus on clear", async () => {
  const view = mount();
  fireEvent.click(screen.getByRole("radio", { name: "Video" }));
  flush();
  expect(view.props.onMediaKindChange).toHaveBeenCalledExactlyOnceWith("video");
  fireEvent.click(screen.getByRole("button", { name: "Show favorites only" }));
  flush();
  expect(view.props.onFavoritesOnlyChange).toHaveBeenCalledExactlyOnceWith(true);
  expect(view.props.onSearchSubmit).not.toHaveBeenCalled();
  view.rerender({ ...view.props, validationError: "groupNameMissingValue" });
  expect(screen.getByRole("alert")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Clear all search filters" }));
  flush();
  expect(view.props.onClearAll).toHaveBeenCalledOnce();
  await waitFor(() => expect(screen.getByRole("combobox")).toHaveFocus());
});
