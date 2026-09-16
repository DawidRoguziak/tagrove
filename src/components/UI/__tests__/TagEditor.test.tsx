import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef, useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TagEditor } from "../TagEditor";

function Editor({ tags = ["cat"], disabled = false, loadingText, removable = true, onAdd = vi.fn(), onRemove = vi.fn() }: {
  tags?: string[];
  disabled?: boolean;
  loadingText?: string;
  removable?: boolean;
  onAdd?: (tag: string) => void;
  onRemove?: (tag: string) => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [draft, setDraft] = useState("");
  return <TagEditor variant="bulk" tags={tags} draft={draft}
    knownTags={["cat", "car"]} inputRef={inputRef} inputId="tag-input"
    inputAriaLabel="Add tag" placeholder="Type to add tag…" listboxAriaLabel="Suggestions"
    onDraftChange={setDraft} onAddTag={onAdd} disabled={disabled} loadingText={loadingText}
    onRemoveTag={removable ? onRemove : undefined} getRemoveTagAriaLabel={tag => `Remove ${tag}`} />;
}

describe("TagEditor", () => {
  afterEach(cleanup);

  it("contains chips followed by the input in one field and focuses from blank space", async () => {
    render(<Editor />);
    const field = screen.getByTestId("bulk-tag-list");
    const input = within(field).getByRole("combobox", { name: "Add tag" });
    expect(within(field).getByText("cat").compareDocumentPosition(input) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(input).toHaveClass("border-0");
    expect(screen.queryByText("Add tag")).not.toBeInTheDocument();
    await userEvent.click(field);
    expect(input).toHaveFocus();
  });

  it("removes through the callback without transferring focus to the input", async () => {
    const onRemove = vi.fn();
    render(<Editor onRemove={onRemove} />);
    await userEvent.click(screen.getByRole("button", { name: "Remove cat" }));
    expect(onRemove).toHaveBeenCalledWith("cat");
    expect(screen.getByRole("combobox")).not.toHaveFocus();
  });

  it.each([{ disabled: true }, { loadingText: "Loading tags..." }])("guards disabled or loading editors: %o", async props => {
    const onAdd = vi.fn();
    const onRemove = vi.fn();
    render(<Editor {...props} onAdd={onAdd} onRemove={onRemove} />);
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
    render(<Editor removable={false} onAdd={onAdd} />);
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
    const { rerender } = render(<Editor onAdd={onAdd} onRemove={onRemove} />);
    await userEvent.type(screen.getByRole("combobox"), "ca");
    await screen.findByRole("option");
    rerender(<Editor disabled onAdd={onAdd} onRemove={onRemove} />);
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(screen.getByRole("combobox")).toHaveAttribute("aria-expanded", "false");
    await userEvent.keyboard("{ArrowDown}{Enter}");
    await userEvent.click(screen.getByRole("button", { name: "Remove cat" }));
    expect(onAdd).not.toHaveBeenCalled();
    expect(onRemove).not.toHaveBeenCalled();
    rerender(<Editor onAdd={onAdd} onRemove={onRemove} />);
    await userEvent.click(screen.getByRole("combobox"));
    await screen.findByRole("option");
    await userEvent.keyboard("{Enter}");
    expect(onAdd).toHaveBeenCalledExactlyOnceWith("ca");
  });

  it("updates exclusions when the caller adds or removes assigned tags", async () => {
    const { rerender } = render(<Editor tags={[]} />);
    await userEvent.type(screen.getByRole("combobox"), "ca");
    await waitFor(() => expect(screen.getAllByRole("option").map(option => option.textContent)).toEqual(["cat", "car"]));
    rerender(<Editor tags={["cat"]} />);
    await waitFor(() => expect(screen.getAllByRole("option").map(option => option.textContent)).toEqual(["car"]));
    rerender(<Editor tags={[]} />);
    await waitFor(() => expect(screen.getAllByRole("option").map(option => option.textContent)).toEqual(["cat", "car"]));
  });

  it("tabs between chip removal and input and dismisses suggestions without losing the draft", async () => {
    render(<Editor />);
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
