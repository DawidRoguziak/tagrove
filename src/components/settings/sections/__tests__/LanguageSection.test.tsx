import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { LanguageSection } from "../LanguageSection";

describe("LanguageSection", () => {
  it("shows the current language and ordered options and notifies about changes", async () => {
    const onLanguageChange = vi.fn();
    render(<LanguageSection language="en" onLanguageChange={onLanguageChange} />);

    expect(screen.getByRole("heading", { name: "Language" })).toBeInTheDocument();
    expect(screen.getByText("Choose application language.")).toBeInTheDocument();
    const languageSelect = screen.getByRole("combobox", { name: "Select language" });
    expect(languageSelect).toHaveValue("en");
    expect(
      within(languageSelect)
        .getAllByRole("option")
        .map((option) => ({ value: (option as HTMLOptionElement).value, label: option.textContent }))
    ).toEqual([
      { value: "en", label: "English" },
      { value: "pl", label: "Polski" },
      { value: "fr", label: "Français" },
      { value: "de", label: "Deutsch" },
      { value: "it", label: "Italiano" },
      { value: "es", label: "Español" },
      { value: "ru", label: "Русский" },
      { value: "zh", label: "中文（简体）" },
      { value: "ja", label: "日本語" },
      { value: "ko", label: "한국어" },
      { value: "cs", label: "Čeština" }
    ]);

    await userEvent.selectOptions(languageSelect, "pl");
    expect(onLanguageChange).toHaveBeenCalledWith("pl");

    await userEvent.selectOptions(languageSelect, "cs");
    expect(onLanguageChange).toHaveBeenLastCalledWith("cs");
  });
});
