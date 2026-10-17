import { cleanup, fireEvent, screen, waitFor, within } from "@solidjs/testing-library";
import { flush, onCleanup } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import i18n, { changeAppLanguage } from "../../../i18n/core";
import { APP_LANGUAGES } from "../../../i18n/languages";
import { renderSolid } from "../../../test/renderSolid";
import { createSearch } from "../createSearch";
import { SearchTagatorWrapper } from "../SearchTagatorWrapper";
import { SuggestionClient } from "../suggestions/SuggestionClient";
import { TagListModal } from "../tag-list/TagListModal";

vi.mock("../../../platform/tauri/tags", () => ({
  listTags: vi.fn().mockResolvedValue({ items: ["alpha", "beta", "gamma"], total: 3 })
}));

afterEach(async () => {
  cleanup();
  await changeAppLanguage("en");
});

describe("search and tag-list translations", () => {
  it.each(APP_LANGUAGES)("renders tag selection and parser errors in %s", async (language) => {
    await changeAppLanguage(language);
    const onApplySearch = vi.fn().mockResolvedValue(undefined);
    const modal = renderSolid(TagListModal, {
      open: true,
      onClose: vi.fn(),
      onApplySearch
    });
    const dialog = within(screen.getByRole("dialog"));
    const alpha = await dialog.findByRole("button", { name: /^alpha/ });
    const beta = dialog.getByRole("button", { name: /^beta/ });
    // The second click commits alpha's delayed include. Shift+Enter then excludes beta.
    fireEvent.click(alpha);
    fireEvent.click(beta);
    fireEvent.keyDown(beta, { key: "Enter", shiftKey: true });
    flush();
    const gamma = dialog.getByRole("button", { name: /^gamma/ });
    expect(alpha).toHaveAttribute("aria-pressed", "true");
    expect(beta).toHaveAttribute("aria-pressed", "true");
    expect(gamma).toHaveAttribute("aria-pressed", "false");
    const filter = dialog.getByRole("textbox");
    const tagList = {
      filter: filter.getAttribute("aria-label"),
      placeholder: filter.getAttribute("placeholder"),
      list: gamma.closest("div[aria-label]")?.getAttribute("aria-label"),
      summary: dialog.getByText(i18n.t("tagList.selectionSummary", { included: 1, excluded: 1 })).textContent,
      states: [alpha, beta, gamma].map((button) => ({
        label: button.getAttribute("aria-label"),
        title: button.getAttribute("title")
      })),
      // These branches have separate model tests; pin their translated copy here too.
      messages: ["loading", "loadingMore", "empty", "noMatches", "applying"].map(
        (key) => i18n.t(`tagList.${key}`)
      )
    };
    fireEvent.click(dialog.getByRole("button", { name: i18n.t("common.apply") }));
    flush();
    await waitFor(() => expect(onApplySearch).toHaveBeenCalledExactlyOnceWith("alpha -beta"));
    modal.unmount();

    const apply = vi.fn().mockResolvedValue(undefined);
    function Search() {
      const search = createSearch(apply);
      const client = new SuggestionClient();
      onCleanup(() => client.dispose());
      return <SearchTagatorWrapper
        client={client}
        filterInput={search.filterInput}
        validationError={search.validationError}
        onFilterChange={search.change}
        onSearchSubmit={() => void search.submit()}
        onApplyTagListSearch={search.applyInput}
        onClearAll={() => void search.clear()}
        knownTags={[]}
        mediaKind={search.mediaKind}
        onMediaKindChange={search.applyKind}
        favoritesOnly={search.favoritesOnly}
        onFavoritesOnlyChange={search.applyFavorites}
      />;
    }
    renderSolid(Search, {});
    const errors: string[] = [];
    for (const input of ["tags cat", "cat,dog", "gN:", "tags:-1"]) {
      fireEvent.input(screen.getByRole("combobox"), { target: { value: input } });
      flush();
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: i18n.t("workspace.search") }));
      flush();
      errors.push(screen.getByRole("alert").textContent ?? "");
    }
    expect(apply).not.toHaveBeenCalled();
    expect(document.documentElement.lang).toBe(language);
    // Exact rendered copy catches English fallback without banning legitimate shared tokens.
    expect({ tagList, errors }).toMatchSnapshot();
  });
});
