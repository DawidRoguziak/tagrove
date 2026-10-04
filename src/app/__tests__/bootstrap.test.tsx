import { render, screen, waitFor } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { isServer } from "@solidjs/web";
import { createSignal, onCleanup, onSettled } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import App from "../App";

describe("Solid browser toolchain", () => {
  it("renders the bootstrap with the browser runtime", () => {
    expect(isServer).toBe(false);
    render(() => <App />);
    expect(screen.getByAltText("Tagrove")).toBeInTheDocument();
    expect(screen.getByTestId("gallery-grid")).toBeInTheDocument();
  });

  it("updates the DOM after an interaction", async () => {
    function Counter() {
      const [count, setCount] = createSignal(0);
      return (
        <button type="button" onClick={() => setCount(count() + 1)}>
          Count {count()}
        </button>
      );
    }
    render(() => <Counter />);
    const button = screen.getByRole("button", { name: "Count 0" });
    await userEvent.click(button);
    expect(button).toHaveTextContent("Count 1");
  });

  it("publishes deferred completion and removes owned listeners on disposal", async () => {
    let complete = (_value: string) => {};
    const completion = new Promise<string>((resolve) => {
      complete = resolve;
    });
    const received = vi.fn();
    const disposed = vi.fn();
    function Deferred() {
      const [message, setMessage] = createSignal("Loading");
      onSettled(() => {
        window.addEventListener("solid-smoke", received);
        void completion.then(setMessage);
      });
      onCleanup(() => {
        window.removeEventListener("solid-smoke", received);
        disposed();
      });
      return <p role="status">{message()}</p>;
    }
    const view = render(() => <Deferred />);
    expect(screen.getByRole("status")).toHaveTextContent("Loading");
    complete("Complete");
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Complete"));
    window.dispatchEvent(new Event("solid-smoke"));
    expect(received).toHaveBeenCalledOnce();
    view.unmount();
    expect(disposed).toHaveBeenCalledOnce();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    window.dispatchEvent(new Event("solid-smoke"));
    expect(received).toHaveBeenCalledOnce();
  });

  it("starts each test with mounted roots cleaned up", () => {
    expect(document.body).toBeEmptyDOMElement();
  });
});
