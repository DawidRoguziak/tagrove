import { render, screen } from "@testing-library/react";
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

    const themeSelect = screen.getByRole("combobox", { name: "Select theme" });
    expect(themeSelect).toHaveValue("dark");

    await userEvent.selectOptions(themeSelect, "light");
    expect(onThemeChange).toHaveBeenCalledWith("light");

    const languageSelect = screen.getByRole("combobox", { name: "Select language" });
    expect(languageSelect).toHaveValue("en");

    await userEvent.selectOptions(languageSelect, "pl");
    expect(onLanguageChange).toHaveBeenCalledWith("pl");

    await userEvent.selectOptions(languageSelect, "cs");
    expect(onLanguageChange).toHaveBeenLastCalledWith("cs");
  });
});
