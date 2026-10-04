import { cleanup, screen, waitFor, within } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { createSignal, onCleanup } from "solid-js";
import { renderSolid } from "../../../test/renderSolid";
import { SuggestionClient } from "../../search/suggestions/SuggestionClient";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TagEditor } from "../TagEditor";

function Editor(props: {
  tags?: string[];
  disabled?: boolean;
  loadingText?: string;
  removable?: boolean;
  onAdd?: (tag: string) => void;
  onRemove?: (tag: string) => void;
}) {
  const [draft, setDraft] = createSignal("");
  const client = new SuggestionClient();
  onCleanup(() => client.dispose());
  return (
    <TagEditor
      variant="bulk"
      tags={props.tags ?? ["cat"]}
      draft={draft()}
      client={client}
      knownTags={["cat", "car"]}
      inputRef={() => {}}
      inputId="tag-input"
      inputAriaLabel="Add tag"
      placeholder="Type to add tag…"
      listboxAriaLabel="Suggestions"
      onDraftChange={setDraft}
      onAddTag={(tag) => props.onAdd?.(tag)}
      disabled={props.disabled}
      loadingText={props.loadingText}
      onRemoveTag={props.removable === false ? undefined : (tag) => props.onRemove?.(tag)}
      getRemoveTagAriaLabel={(tag) => `Remove ${tag}`}
    />
  );
}

describe("TagEditor", () => {
  afterEach(cleanup);

  it("contains chips followed by the input in one field and focuses from blank space", async () => {
    renderSolid(Editor, {});
    const field = screen.getByTestId("bulk-tag-list");
    const input = within(field).getByRole("combobox", { name: "Add tag" });
    expect(
      within(field).getByText("cat").compareDocumentPosition(input) &
        Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();
    expect(input).toHaveClass("border-0");
    expect(screen.queryByText("Add tag")).not.toBeInTheDocument();
    await userEvent.click(field);
    expect(input).toHaveFocus();
  });

  it("removes through the callback without transferring focus to the input", async () => {
    const onRemove = vi.fn();
    renderSolid(Editor, { onRemove: onRemove });
    await userEvent.click(screen.getByRole("button", { name: "Remove cat" }));
    expect(onRemove).toHaveBeenCalledWith("cat");
    expect(screen.getByRole("combobox")).not.toHaveFocus();
  });

  it.each([
    { disabled: true },
    { loadingText: "Loading tags..." }
  ])("guards disabled or loading editors: %o", async (props) => {
    const onAdd = vi.fn();
    const onRemove = vi.fn();
    renderSolid(Editor, { onAdd: onAdd, onRemove: onRemove, ...props });
    await userEvent.click(screen.getByTestId("bulk-tag-list"));
    expect(screen.getByRole("combobox")).toBeDisabled();
    expect(screen.getByRole("combobox")).not.toHaveFocus();
    for (const button of screen.queryAllByRole("button")) {
      expect(button).toBeDisabled();
      await userEvent.click(button);
    }
    await userEvent.keyboard("dog{Enter}");
    expect(onAdd).not.toHaveBeenCalled();
    expect(onRemove).not.toHaveBeenCalled();
  });

  it("supports add-only chips, excludes assigned tags and submits typed or selected tags", async () => {
    const onAdd = vi.fn();
    renderSolid(Editor, { removable: false, onAdd: onAdd });
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    await userEvent.type(screen.getByRole("combobox"), "ca");
    expect(await screen.findByRole("option")).toHaveTextContent("car");
    expect(screen.queryByRole("option", { name: "cat" })).not.toBeInTheDocument();
    await userEvent.keyboard("{Enter}");
    expect(onAdd).toHaveBeenLastCalledWith("ca");
    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(onAdd).toHaveBeenLastCalledWith("car");
  });
  it("closes active suggestions and blocks writes when disabled, then recovers", async () => {
    const onAdd = vi.fn();
    const onRemove = vi.fn();
    const { rerender } = renderSolid(Editor, { onAdd: onAdd, onRemove: onRemove });
    await userEvent.type(screen.getByRole("combobox"), "ca");
    await screen.findByRole("option");
    rerender({ onAdd: onAdd, onRemove: onRemove, disabled: true });
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(screen.getByRole("combobox")).toHaveAttribute("aria-expanded", "false");
    await userEvent.keyboard("{ArrowDown}{Enter}");
    await userEvent.click(screen.getByRole("button", { name: "Remove cat" }));
    expect(onAdd).not.toHaveBeenCalled();
    expect(onRemove).not.toHaveBeenCalled();
    rerender({ onAdd: onAdd, onRemove: onRemove });
    await userEvent.click(screen.getByRole("combobox"));
    await screen.findByRole("option");
    await userEvent.keyboard("{Enter}");
    expect(onAdd).toHaveBeenCalledExactlyOnceWith("ca");
  });

  it("updates exclusions when the caller adds or removes assigned tags", async () => {
    const { rerender } = renderSolid(Editor, { tags: [] });
    await userEvent.type(screen.getByRole("combobox"), "ca");
    await waitFor(() =>
      expect(screen.getAllByRole("option").map((option) => option.textContent)).toEqual([
        "cat",
        "car"
      ])
    );
    rerender({ tags: ["cat"] });
    await waitFor(() =>
      expect(screen.getAllByRole("option").map((option) => option.textContent)).toEqual(["car"])
    );
    rerender({ tags: [] });
    await waitFor(() =>
      expect(screen.getAllByRole("option").map((option) => option.textContent)).toEqual([
        "cat",
        "car"
      ])
    );
  });

  it("tabs between chip removal and input and dismisses suggestions without losing the draft", async () => {
    renderSolid(Editor, {});
    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Remove cat" })).toHaveFocus();
    await userEvent.tab();
    expect(screen.getByRole("combobox")).toHaveFocus();
    await userEvent.keyboard("ca");
    await screen.findByRole("option");
    await userEvent.keyboard("{ArrowDown}{Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(screen.getByRole("combobox")).toHaveFocus();
    expect(screen.getByRole("combobox")).toHaveValue("ca");
    await userEvent.tab({ shift: true });
    expect(screen.getByRole("button", { name: "Remove cat" })).toHaveFocus();
  });
});
