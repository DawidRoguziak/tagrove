import { cleanup, screen, waitFor } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { createSignal, onCleanup } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { renderSolid } from "../../../test/renderSolid";
import { TagEditor } from "../../tagging/TagEditor";
import { SearchTagator } from "../SearchTagator";
import { SuggestionClient } from "../suggestions/SuggestionClient";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

function RecoveryEditor(props: { variant: "search" | "lightbox" | "bulk"; submit: (tag: string) => void }) {
  const [value, setValue] = createSignal("");
  let fail = true;
  const client = new SuggestionClient(() => {
    if (fail) {
      fail = false;
      throw new Error("worker unavailable");
    }
    return new Worker("suggestions-test");
  });
  onCleanup(() => client.dispose());
  const knownTags = ["cat"];
  const tags: string[] = [];
  return <>{props.variant === "search" ? (
    <SearchTagator client={client} value={value()} onValueChange={setValue}
      knownTags={knownTags} onSubmit={() => props.submit(value())} />
  ) : (
    <TagEditor variant={props.variant} tags={tags} draft={value()} client={client}
      knownTags={knownTags} inputRef={() => {}} inputId="recovery-input"
      inputAriaLabel="Add tag" placeholder="Add tag" listboxAriaLabel="Suggestions"
      onDraftChange={setValue} onAddTag={props.submit} />
  )}</>;
}

for (const variant of ["search", "lightbox", "bulk"] as const) {
  it.each(["pointer", "keyboard"])(`${variant} recovers suggestions after %s focus transfer`, async (method) => {
    const user = userEvent.setup();
    const submit = vi.fn();
    renderSolid(RecoveryEditor, { variant, submit });
    const input = screen.getByRole("combobox");
    await user.type(input, "c");
    const retry = await screen.findByRole("button", { name: "Suggestions unavailable. Retry" });
    if (method === "pointer") {
      await user.pointer({ target: retry, keys: "[MouseLeft>]" });
      expect(retry).toBeInTheDocument();
      await user.pointer({ keys: "[/MouseLeft]" });
    } else {
      await user.tab();
      expect(retry).toHaveFocus();
      await user.keyboard("{Enter}");
    }
    expect(await screen.findByRole("option")).toHaveTextContent("cat");
    await waitFor(() => expect(input).toHaveFocus());
    expect(submit).not.toHaveBeenCalled();
    await user.keyboard("{Escape}{Enter}");
    expect(submit).toHaveBeenCalledExactlyOnceWith("c");
  });
}
