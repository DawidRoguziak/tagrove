import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AppearanceSection } from "../AppearanceSection";

describe("AppearanceSection", () => {
  it("shows current theme and notifies about changes", async () => {
    const onThemeChange = vi.fn();

    render(
      <AppearanceSection
        theme="dark"
        onThemeChange={onThemeChange}
      />
    );

    expect(screen.getByRole("radio", { name: "Dark" })).toBeChecked();
    expect(screen.getByRole("group", { name: "Select theme" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("radio", { name: "Light" }));
    expect(onThemeChange).toHaveBeenCalledWith("light");

    expect(screen.getByText("Choose application theme.")).toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  });
});
