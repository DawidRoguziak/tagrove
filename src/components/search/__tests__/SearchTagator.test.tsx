import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SearchTagator } from "../SearchTagator";

function ControlledSearchTagator({
  knownTags,
  initialValue
}: {
  knownTags: string[];
  initialValue: string;
}) {
  const [value, setValue] = useState(initialValue);

  return (
    <SearchTagator
      value={value}
      onValueChange={setValue}
      knownTags={knownTags}
      ariaLabel="Search tags"
      listboxAriaLabel="Tag suggestions"
    />
  );
}

describe("SearchTagator", () => {
  afterEach(() => {
    cleanup();
  });

  it("shows suggestions and applies selected suggestion with keyboard", async () => {
    render(<ControlledSearchTagator knownTags={["cat", "car", "dog"]} initialValue="ca" />);

    const input = screen.getByRole("textbox", { name: "Search tags" }) as HTMLInputElement;
    await userEvent.click(input);

    await waitFor(() => {
      expect(screen.getByRole("listbox", { name: "Tag suggestions" })).toBeInTheDocument();
    });

    await userEvent.keyboard("{ArrowDown}{Enter}");

    await waitFor(() => {
      expect(input.value).toMatch(/^(cat|car)$/);
    });
  });

  it("positions suggestions below the input by default", async () => {
    render(<ControlledSearchTagator knownTags={["cat"]} initialValue="ca" />);

    await userEvent.click(screen.getByRole("textbox", { name: "Search tags" }));

    const listbox = await screen.findByRole("listbox", { name: "Tag suggestions" });
    expect(listbox).toHaveClass("top-[calc(100%+8px)]");
    expect(listbox).not.toHaveClass("bottom-[calc(100%+8px)]");
  });

  it("can position suggestions above the input", async () => {
    render(
      <SearchTagator
        value="ca"
        onValueChange={vi.fn()}
        knownTags={["cat"]}
        suggestionsPlacement="above"
        ariaLabel="Search tags"
        listboxAriaLabel="Tag suggestions"
      />
    );

    await userEvent.click(screen.getByRole("textbox", { name: "Search tags" }));

    const listbox = await screen.findByRole("listbox", { name: "Tag suggestions" });
    expect(listbox).toHaveClass("bottom-[calc(100%+8px)]");
    expect(listbox).not.toHaveClass("top-[calc(100%+8px)]");
  });

  it("keeps negative prefix when suggestion replaces active token", async () => {
    render(<ControlledSearchTagator knownTags={["cat"]} initialValue="-ca" />);

    const input = screen.getByRole("textbox", { name: "Search tags" }) as HTMLInputElement;
    await userEvent.click(input);

    await waitFor(() => {
      expect(screen.getByRole("listbox", { name: "Tag suggestions" })).toBeInTheDocument();
    });

    await userEvent.keyboard("{Enter}");

    await waitFor(() => {
      expect(input.value).toBe("-cat");
    });
  });

  it("keeps suggestion list closed when input is empty", async () => {
    render(<ControlledSearchTagator knownTags={["cat", "car"]} initialValue="" />);

    const input = screen.getByRole("textbox", { name: "Search tags" }) as HTMLInputElement;
    await userEvent.click(input);

    expect(screen.queryByRole("listbox", { name: "Tag suggestions" })).not.toBeInTheDocument();

    await userEvent.type(input, "ca");

    await waitFor(() => {
      expect(screen.getByRole("listbox", { name: "Tag suggestions" })).toBeInTheDocument();
    });

    await userEvent.clear(input);

    await waitFor(() => {
      expect(screen.queryByRole("listbox", { name: "Tag suggestions" })).not.toBeInTheDocument();
    });
  });

  it("closes suggestion list after input loses focus", async () => {
    render(
      <div>
        <ControlledSearchTagator knownTags={["cat", "car"]} initialValue="ca" />
        <button type="button">Outside</button>
      </div>
    );

    const input = screen.getByRole("textbox", { name: "Search tags" });
    await userEvent.click(input);

    await waitFor(() => {
      expect(screen.getByRole("listbox", { name: "Tag suggestions" })).toBeInTheDocument();
    });

    await userEvent.click(screen.getByRole("button", { name: "Outside" }));

    await waitFor(() => {
      expect(screen.queryByRole("listbox", { name: "Tag suggestions" })).not.toBeInTheDocument();
    });
  });

  it("calls onSubmit when pressing enter without suggestions", async () => {
    const onSubmit = vi.fn();

    render(
      <SearchTagator
        value="bird"
        onValueChange={vi.fn()}
        knownTags={[]}
        onSubmit={onSubmit}
        ariaLabel="Search tags"
      />
    );

    const input = screen.getByRole("textbox", { name: "Search tags" });
    await userEvent.click(input);
    await userEvent.keyboard("{Enter}");

    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("uses onSuggestionPick callback and keeps suggestions open on pick", async () => {
    const onSuggestionPick = vi.fn();

    render(
      <SearchTagator
        value="ca"
        onValueChange={vi.fn()}
        knownTags={["cat", "car"]}
        onSuggestionPick={onSuggestionPick}
        keepSuggestionsOpenOnPick
        ariaLabel="Search tags"
        listboxAriaLabel="Tag suggestions"
      />
    );

    const input = screen.getByRole("textbox", { name: "Search tags" });
    await userEvent.click(input);

    await waitFor(() => {
      expect(screen.getByRole("listbox", { name: "Tag suggestions" })).toBeInTheDocument();
    });

    await userEvent.keyboard("{Enter}");

    expect(onSuggestionPick).toHaveBeenCalledTimes(1);
    expect(onSuggestionPick.mock.calls[0]?.[0]).toMatch(/^ca/);

    await waitFor(() => {
      expect(screen.getByRole("listbox", { name: "Tag suggestions" })).toBeInTheDocument();
    });
  });

  it("keeps keyboard-selected option highlighted after enter when list stays open", async () => {
    const onSuggestionPick = vi.fn();

    render(
      <SearchTagator
        value="ca"
        onValueChange={vi.fn()}
        knownTags={["cat", "car", "castle"]}
        onSuggestionPick={onSuggestionPick}
        keepSuggestionsOpenOnPick
        ariaLabel="Search tags"
        listboxAriaLabel="Tag suggestions"
      />
    );

    const input = screen.getByRole("textbox", { name: "Search tags" });
    await userEvent.click(input);

    const listbox = await screen.findByRole("listbox", { name: "Tag suggestions" });
    let options = within(listbox).getAllByRole("option");
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(options[0]).toHaveStyle("background-color: var(--tagator-suggestion-active-bg)");

    await userEvent.keyboard("{ArrowDown}");
    options = within(listbox).getAllByRole("option");
    expect(options[1]).toHaveAttribute("aria-selected", "true");
    expect(options[1]).toHaveStyle("background-color: var(--tagator-suggestion-active-bg)");

    await userEvent.keyboard("{Enter}");
    expect(onSuggestionPick).toHaveBeenCalledTimes(1);

    await waitFor(() => {
      const updatedOptions = within(screen.getByRole("listbox", { name: "Tag suggestions" })).getAllByRole("option");
      expect(updatedOptions[1]).toHaveAttribute("aria-selected", "true");
      expect(updatedOptions[1]).toHaveStyle("background-color: var(--tagator-suggestion-active-bg)");
    });
  });

  it("does not auto-select first suggestion when autoSelectFirstSuggestion is false", async () => {
    const onSuggestionPick = vi.fn();
    const onSubmit = vi.fn();

    render(
      <SearchTagator
        value="ca"
        onValueChange={vi.fn()}
        knownTags={["cat", "car"]}
        onSuggestionPick={onSuggestionPick}
        onSubmit={onSubmit}
        autoSelectFirstSuggestion={false}
        ariaLabel="Search tags"
        listboxAriaLabel="Tag suggestions"
      />
    );

    const input = screen.getByRole("textbox", { name: "Search tags" });
    await userEvent.click(input);

    const listbox = await screen.findByRole("listbox", { name: "Tag suggestions" });
    const options = within(listbox).getAllByRole("option");
    expect(options[0]).toHaveAttribute("aria-selected", "false");
    expect(options[1]).toHaveAttribute("aria-selected", "false");

    await userEvent.keyboard("{Enter}");

    expect(onSuggestionPick).not.toHaveBeenCalled();
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("still allows keyboard picking when autoSelectFirstSuggestion is false", async () => {
    const onSuggestionPick = vi.fn();

    render(
      <SearchTagator
        value="ca"
        onValueChange={vi.fn()}
        knownTags={["cat", "car"]}
        onSuggestionPick={onSuggestionPick}
        autoSelectFirstSuggestion={false}
        ariaLabel="Search tags"
        listboxAriaLabel="Tag suggestions"
      />
    );

    const input = screen.getByRole("textbox", { name: "Search tags" });
    await userEvent.click(input);

    await screen.findByRole("listbox", { name: "Tag suggestions" });
    await userEvent.keyboard("{ArrowDown}{Enter}");

    expect(onSuggestionPick).toHaveBeenCalledTimes(1);
    expect(onSuggestionPick.mock.calls[0]?.[0]).toMatch(/^ca/);
  });

  it("does not open suggestions for has-no-tags metatag input", async () => {
    render(<ControlledSearchTagator knownTags={["tags", "tagstone", "cat"]} initialValue="tags" />);

    const input = screen.getByRole("textbox", { name: "Search tags" });
    await userEvent.click(input);

    await waitFor(() => {
      expect(screen.queryByRole("listbox", { name: "Tag suggestions" })).not.toBeInTheDocument();
    });

    expect((input as HTMLInputElement).value).toBe("tags");
  });

  it("does not open suggestions for group-name metatag input", async () => {
    render(
      <ControlledSearchTagator
        knownTags={["trip-2026", "trip-2027", "cat"]}
        initialValue="gN:trip-2026"
      />
    );

    const input = screen.getByRole("textbox", { name: "Search tags" });
    await userEvent.click(input);

    await waitFor(() => {
      expect(screen.queryByRole("listbox", { name: "Tag suggestions" })).not.toBeInTheDocument();
    });

    expect((input as HTMLInputElement).value).toBe("gN:trip-2026");
  });
});

