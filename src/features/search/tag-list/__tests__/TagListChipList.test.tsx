import { renderSolid } from "../../../../test/renderSolid";
import { settle as act } from "../../../../test/settle";
import { cleanup } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { changeAppLanguage } from "../../../../i18n/core";
import { useLanguage } from "../../../../i18n/solid";
import { TagListChipList } from "../TagListChipList";
import type { TagSelections } from "../tagListSelectionService";

function selections(included: string[] = [], excluded: string[] = []): TagSelections {
  return {
    included: new Map(included.map((tag) => [tag.toLowerCase(), tag])),
    excluded: new Map(excluded.map((tag) => [tag.toLowerCase(), tag]))
  };
}

const counts = { state: vi.fn(), title: vi.fn(), aria: vi.fn() };

function Harness(props: { tags: string[]; selections: TagSelections }) {
  const { t } = useLanguage();
  return (
    <TagListChipList
      tags={props.tags}
      selections={props.selections}
      disabled={false}
      onTagClick={vi.fn()}
      onTagDoubleClick={vi.fn()}
      getStateLabel={(mode) => {
        counts.state(mode);
        return t(
          `tagList.states.${mode === "include" ? "included" : mode === "exclude" ? "excluded" : "inactive"}`
        );
      }}
      getButtonTitle={(tag, state) => {
        counts.title(tag);
        return t("tagList.tagButtonTitle", { tag, state });
      }}
      getButtonAriaLabel={(tag, state) => {
        counts.aria(tag);
        return t("tagList.tagButtonAria", { tag, state });
      }}
    />
  );
}

afterEach(async () => {
  cleanup();
  vi.clearAllMocks();
  await changeAppLanguage("en");
});

describe("TagListChipList", () => {
  const tags = ["Alpha", "Beta", "Gamma"];

  it("recomputes translated strings only for the chip whose mode changed", () => {
    const { rerender, getByRole } = renderSolid(Harness, { tags, selections: selections() });
    expect(counts.aria).toHaveBeenCalledTimes(3);
    expect(counts.title).toHaveBeenCalledTimes(3);
    expect(counts.state).toHaveBeenCalledTimes(3);
    vi.clearAllMocks();

    rerender({ tags, selections: selections(["Beta"]) });

    expect(counts.state).toHaveBeenCalledTimes(1);
    expect(counts.aria).toHaveBeenCalledTimes(1);
    expect(counts.aria).toHaveBeenCalledWith("Beta");
    expect(counts.title).toHaveBeenCalledTimes(1);
    expect(getByRole("button", { name: "Beta, state: included" })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
    expect(getByRole("button", { name: "Alpha, state: inactive" })).toHaveAttribute(
      "aria-pressed",
      "false"
    );

    vi.clearAllMocks();
    rerender({ tags, selections: selections([], ["Beta"]) });
    expect(counts.aria).toHaveBeenCalledTimes(1);
    expect(getByRole("button", { name: "Beta, state: excluded" })).toHaveAttribute(
      "title",
      "Beta (excluded)"
    );
  });

  it("does not recompute anything when an equal selection is published", () => {
    const { rerender } = renderSolid(Harness, { tags, selections: selections(["Alpha"]) });
    vi.clearAllMocks();
    rerender({ tags, selections: selections(["Alpha"]) });
    expect(counts.state).not.toHaveBeenCalled();
    expect(counts.aria).not.toHaveBeenCalled();
    expect(counts.title).not.toHaveBeenCalled();
  });

  it("follows a language change", async () => {
    const { getByRole } = renderSolid(Harness, { tags, selections: selections(["Alpha"]) });
    expect(getByRole("button", { name: "Alpha, state: included" })).toBeInTheDocument();
    await act(async () => {
      await changeAppLanguage("pl");
    });
    const button = getByRole("button", { name: /^Alpha, stan: / });
    expect(button).toHaveAttribute("title", expect.stringMatching(/^Alpha \(/));
    expect(button).toHaveAttribute("aria-pressed", "true");
  });
});
