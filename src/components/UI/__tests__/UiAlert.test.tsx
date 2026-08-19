import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { UiAlert } from "../UiAlert";

describe("UiAlert", () => {
  it("renders warning alert with title and content", () => {
    render(
      <UiAlert tone="warning" title="Overwrite warning">
        Existing values will be replaced.
      </UiAlert>
    );

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Overwrite warning");
    expect(alert).toHaveTextContent("Existing values will be replaced.");
    expect(alert).toHaveClass("border-warning/52");
    expect(alert).toHaveClass("text-base-content");
  });

  it("renders tone-specific border/background classes for info and error", () => {
    const { rerender } = render(<UiAlert tone="info">Info content</UiAlert>);
    const infoAlert = screen.getByText("Info content").closest("[role='alert']");

    expect(infoAlert).not.toBeNull();
    expect(infoAlert).toHaveClass("border-info/52", "bg-info/14");

    rerender(<UiAlert tone="error">Error content</UiAlert>);
    const errorAlert = screen.getByText("Error content").closest("[role='alert']");

    expect(errorAlert).not.toBeNull();
    expect(errorAlert).toHaveClass("border-error/52", "bg-error/14");
  });
});
