import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AppearanceSection } from "../AppearanceSection";

describe("AppearanceSection", () => {
  it("shows current theme and notifies about changes", async () => {
    const onThemeChange = vi.fn();
    const onLanguageChange = vi.fn();

    render(
      <AppearanceSection
        theme="dark"
        onThemeChange={onThemeChange}
        language="en"
        onLanguageChange={onLanguageChange}
      />
    );

    expect(screen.getByRole("radio", { name: "Dark" })).toBeChecked();
    expect(screen.getByRole("group", { name: "Select theme" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("radio", { name: "Light" }));
    expect(onThemeChange).toHaveBeenCalledWith("light");

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
