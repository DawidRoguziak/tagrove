import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import i18n, { changeAppLanguage } from "../../../i18n";
import { APP_LANGUAGES } from "../../../i18n/languages";
import { GalleryStatusFooter } from "../GalleryStatusFooter";

afterEach(async () => {
  cleanup();
  await changeAppLanguage("en");
});

describe("GalleryStatusFooter localization", () => {
  it.each(APP_LANGUAGES)("formats counts in %s using its plural rules", async (language) => {
    await changeAppLanguage(language);
    const props = { isGeneratingThumbnails: false, hasMore: false, generatingLabel: "" };
    const { getByTestId, rerender } = render(<GalleryStatusFooter {...props} resultCount={1} />);
    for (const count of [1, 2, 5, 21, 22, 30187, 1000000]) {
      rerender(<GalleryStatusFooter {...props} resultCount={count} />);
      const category = new Intl.PluralRules(language).select(count);
      const template: unknown = i18n.getResource(language, "translation", `gallery.resultCount_${category}`);
      expect(typeof template).toBe("string");
      if (typeof template !== "string") throw new Error("Missing plural translation");
      expect(getByTestId("gallery-end-state").querySelector("p")?.textContent).toBe(
        template.replace("{{formattedCount}}", new Intl.NumberFormat(language).format(count))
      );
    }
  });

  it("updates an already mounted footer when language changes", async () => {
    const props = { isGeneratingThumbnails: false, hasMore: false, generatingLabel: "" };
    const { getByRole, getByText, rerender } = render(<GalleryStatusFooter {...props} resultCount={30187} />);
    expect(getByText("30,187 items")).toBeInTheDocument();
    await act(() => changeAppLanguage("pl"));
    expect(getByRole("heading", { name: "Koniec wyników" })).toBeInTheDocument();
    expect(getByText("30 187 elementów")).toBeInTheDocument();
    for (const [count, label] of [[1, "1 element"], [2, "2 elementy"], [5, "5 elementów"], [22, "22 elementy"]] as const) {
      rerender(<GalleryStatusFooter {...props} resultCount={count} />);
      expect(getByText(label)).toBeInTheDocument();
    }
  });
});
