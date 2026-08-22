import { lazy, Suspense } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LazyErrorBoundary } from "../LazyErrorBoundary";

describe("LazyErrorBoundary", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows a recoverable fallback when a lazy chunk rejects", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const preventReportedError = (event: ErrorEvent) => event.preventDefault();
    window.addEventListener("error", preventReportedError);
    const FailedChunk = lazy(() => Promise.reject(new Error("chunk unavailable")));

    render(
      <LazyErrorBoundary resetKey="settings" fallback={<button type="button">Retry loading</button>}>
        <Suspense fallback={<span>Loading chunk</span>}>
          <FailedChunk />
        </Suspense>
      </LazyErrorBoundary>
    );

    expect(await screen.findByRole("button", { name: "Retry loading" })).toBeInTheDocument();
    window.removeEventListener("error", preventReportedError);
  });
});
