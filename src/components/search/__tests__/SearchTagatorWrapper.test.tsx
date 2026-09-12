import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SearchTagatorWrapper } from "../SearchTagatorWrapper";

const mockedSearchTagator = vi.hoisted(() => vi.fn());

vi.mock("../SearchTagator", () => ({
  SearchTagator: (props: {
    value: string;
    placeholder?: string;
    inputRef?: unknown;
    onValueChange: (value: string) => void;
    onSubmit?: () => void;
  }) => {
    mockedSearchTagator(props);
    return (
      <input
        aria-label="Mock search input"
        placeholder={props.placeholder}
        value={props.value}
        ref={(node) => {
          if (props.inputRef) {
            (props.inputRef as { current: HTMLInputElement | null }).current = node;
          }
        }}
        onChange={(event) => {
          props.onValueChange(event.target.value);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            props.onSubmit?.();
          }
        }}
      />
    );
  }
}));

describe("SearchTagatorWrapper", () => {
  afterEach(() => {
    cleanup();
    mockedSearchTagator.mockClear();
  });

  it.each(["ctrlKey", "metaKey"] as const)("focuses the preserved draft with %s+K without submitting, and respects modals", (modifier) => {
    const onSearchSubmit = vi.fn();
    const onFilterChange = vi.fn();
    render(<SearchTagatorWrapper filterInput="cat -dog" onFilterChange={onFilterChange}
      knownTags={[]} mediaKind="all" onMediaKindChange={vi.fn()} onSearchSubmit={onSearchSubmit}
      onClearAll={vi.fn()} favoritesOnly={false} onFavoritesOnlyChange={vi.fn()} />);
    const input = screen.getByRole("textbox", { name: "Mock search input" });
    fireEvent.keyDown(window, { key: "k", [modifier]: true });
    expect(input).toHaveFocus();
    expect(input).toHaveValue("cat -dog");
    expect(onSearchSubmit).not.toHaveBeenCalled();
    expect(onFilterChange).not.toHaveBeenCalled();
    const modal = document.createElement("div");
    modal.setAttribute("role", "dialog");
    modal.setAttribute("aria-modal", "true");
    modal.tabIndex = -1;
    document.body.append(modal);
    modal.focus();
    fireEvent.keyDown(window, { key: "k", [modifier]: true });
    expect(modal).toHaveFocus();
    modal.remove();
  });

  it("changes media kind and submits after parent applies new media kind", async () => {
    const onFilterChange = vi.fn();
    const onMediaKindChange = vi.fn();
    const onSearchSubmit = vi.fn();

    const { rerender } = render(
      <SearchTagatorWrapper
        filterInput="cat"
        onFilterChange={onFilterChange}
        knownTags={[]}
        mediaKind="all"
        onMediaKindChange={onMediaKindChange}
        onSearchSubmit={onSearchSubmit}
        onClearAll={vi.fn()}
        favoritesOnly={false}
        onFavoritesOnlyChange={vi.fn()}
      />
    );

    await userEvent.click(screen.getByRole("radio", { name: "Video" }));

    expect(onMediaKindChange).toHaveBeenCalledWith("video");
    expect(onSearchSubmit).not.toHaveBeenCalled();

    rerender(
      <SearchTagatorWrapper
        filterInput="cat"
        onFilterChange={onFilterChange}
        knownTags={[]}
        mediaKind="video"
        onMediaKindChange={onMediaKindChange}
        onSearchSubmit={onSearchSubmit}
        onClearAll={vi.fn()}
        favoritesOnly={false}
        onFavoritesOnlyChange={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(onSearchSubmit).toHaveBeenCalledTimes(1);
    });
  });

  it("calls onClearAll and returns focus to search input", async () => {
    const onClearAll = vi.fn();

    render(
      <SearchTagatorWrapper
        filterInput="cat"
        onFilterChange={vi.fn()}
        knownTags={[]}
        mediaKind="all"
        onMediaKindChange={vi.fn()}
        onSearchSubmit={vi.fn()}
        onClearAll={onClearAll}
        favoritesOnly={false}
        onFavoritesOnlyChange={vi.fn()}
      />
    );

    const input = screen.getByLabelText("Mock search input");
    await userEvent.click(screen.getByRole("button", { name: "Clear all search filters" }));

    expect(onClearAll).toHaveBeenCalledTimes(1);

    await waitFor(() => {
      expect(input).toHaveFocus();
    });
  });

  it("forwards submit from search input", async () => {
    const onSearchSubmit = vi.fn();

    render(
      <SearchTagatorWrapper
        filterInput="cat"
        onFilterChange={vi.fn()}
        knownTags={[]}
        mediaKind="all"
        onMediaKindChange={vi.fn()}
        onSearchSubmit={onSearchSubmit}
        onClearAll={vi.fn()}
        favoritesOnly={false}
        onFavoritesOnlyChange={vi.fn()}
      />
    );

    const input = screen.getByLabelText("Mock search input");
    await userEvent.click(input);
    await userEvent.keyboard("{Enter}");

    expect(onSearchSubmit).toHaveBeenCalledTimes(1);
  });

  it("toggles favorites-only and submits after parent applies new value", async () => {
    const onFavoritesOnlyChange = vi.fn();
    const onSearchSubmit = vi.fn();

    const { rerender } = render(
      <SearchTagatorWrapper
        filterInput="cat"
        onFilterChange={vi.fn()}
        knownTags={[]}
        mediaKind="all"
        onMediaKindChange={vi.fn()}
        onSearchSubmit={onSearchSubmit}
        onClearAll={vi.fn()}
        favoritesOnly={false}
        onFavoritesOnlyChange={onFavoritesOnlyChange}
      />
    );

    await userEvent.click(screen.getByRole("button", { name: "Show favorites only" }));

    expect(onFavoritesOnlyChange).toHaveBeenCalledWith(true);
    expect(onSearchSubmit).not.toHaveBeenCalled();

    rerender(
      <SearchTagatorWrapper
        filterInput="cat"
        onFilterChange={vi.fn()}
        knownTags={[]}
        mediaKind="all"
        onMediaKindChange={vi.fn()}
        onSearchSubmit={onSearchSubmit}
        onClearAll={vi.fn()}
        favoritesOnly
        onFavoritesOnlyChange={onFavoritesOnlyChange}
      />
    );

    await waitFor(() => {
      expect(onSearchSubmit).toHaveBeenCalledTimes(1);
    });
  });
});
