import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { SectionOperationState } from "../types";
import { SectionOperationStatus } from "../SectionOperationStatus";

describe("SectionOperationStatus", () => {
  afterEach(() => {
    cleanup();
  });

  it("renders spinner status while section is loading", () => {
    const state: SectionOperationState = {
      loading: true,
      message: "Scanning...",
      progress: null
    };

    render(<SectionOperationStatus state={state} loaderTestId="scan-loader" />);

    expect(screen.getByRole("status")).toHaveTextContent("Scanning...");
    expect(screen.getByTestId("scan-loader")).toBeInTheDocument();
  });

  it("renders progress bar with formatted label", () => {
    const state: SectionOperationState = {
      loading: false,
      message: "In progress",
      progress: {
        phase: "scanning",
        processed: 4,
        total: 10,
        message: "Scanning 4/10"
      }
    };

    render(<SectionOperationStatus state={state} loaderTestId="scan-loader" />);

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByText("In progress")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("value", "4");
    expect(screen.getByRole("progressbar")).toHaveAttribute("max", "10");
    expect(screen.getByText("4/10")).toBeInTheDocument();
  });

  it("renders indeterminate progress when total equals zero", () => {
    const state: SectionOperationState = {
      loading: false,
      message: "Waiting",
      progress: {
        phase: "counting",
        processed: 0,
        total: 0,
        message: "Counting"
      }
    };

    render(<SectionOperationStatus state={state} loaderTestId="scan-loader" />);

    const progress = screen.getByRole("progressbar");
    expect(progress).toHaveAttribute("max", "1");
    expect(progress).not.toHaveAttribute("value");
  });
});
