import { useState } from "react";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { UiSegmentedControl } from "../UiSegmentedControl";

const options = [
  { value: "all", label: "All" },
  { value: "image", label: "Images" },
  { value: "video", label: "Video" }
] as const;

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("UiSegmentedControl", () => {
  it("names the group and radios without exposing the width-reserving copies", () => {
    render(<UiSegmentedControl options={options} value="all" onChange={vi.fn()} label="Media type" id="media" className="toolbar-control" />);
    const group = screen.getByRole("group", { name: "Media type" });
    expect(group).toHaveAttribute("id", "media");
    expect(group).toHaveClass("toolbar-control");
    expect(within(group).getAllByRole("radio")).toHaveLength(3);
    for (const option of options) {
      expect(within(group).getByRole("radio", { name: option.label })).toHaveAccessibleName(option.label);
    }
  });

  it("requests selection and waits for the controlled value, including external updates", async () => {
    const onChange = vi.fn();
    const { rerender } = render(<UiSegmentedControl options={options} value="all" onChange={onChange} label="Media" />);
    await userEvent.click(screen.getByRole("radio", { name: "Video" }));
    expect(onChange).toHaveBeenCalledExactlyOnceWith("video");
    expect(screen.getByRole("radio", { name: "All" })).toBeChecked();
    rerender(<UiSegmentedControl options={options} value="video" onChange={onChange} label="Media" />);
    expect(screen.getByRole("radio", { name: "Video" })).toBeChecked();
    rerender(<UiSegmentedControl options={options} value="image" onChange={onChange} label="Media" />);
    expect(screen.getByRole("radio", { name: "Images" })).toBeChecked();
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("keeps radio groups independent and retains native keyboard selection", async () => {
    function Control({ label }: { label: string }) {
      const [value, setValue] = useState<"all" | "image" | "video">("all");
      return <UiSegmentedControl options={options} value={value} onChange={setValue} label={label} />;
    }
    render(<><Control label="First" /><Control label="Second" /></>);
    const first = within(screen.getByRole("group", { name: "First" }));
    const second = within(screen.getByRole("group", { name: "Second" }));
    expect(first.getByRole("radio", { name: "All" }).getAttribute("name"))
      .not.toBe(second.getByRole("radio", { name: "All" }).getAttribute("name"));
    const user = userEvent.setup();
    await user.tab();
    expect(first.getByRole("radio", { name: "All" })).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(first.getByRole("radio", { name: "Images" })).toBeChecked();
    expect(first.getByRole("radio", { name: "Images" })).toHaveFocus();
    expect(second.getByRole("radio", { name: "All" })).toBeChecked();
    await user.tab();
    expect(second.getByRole("radio", { name: "All" })).toHaveFocus();
  });

  it("places immediately, slides external selection, snaps changed layout and cleans up observers", () => {
    let optionWidth = 70;
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      const index = options.findIndex(option => this.querySelector("input")?.value === option.value);
      return this.tagName === "LABEL"
        ? new DOMRect(index * optionWidth + 3, 3, optionWidth, 26)
        : new DOMRect(0, 0, 250, 32);
    });
    let notifyResize = () => {};
    const observer = { observe: vi.fn(), unobserve: vi.fn(), disconnect: vi.fn() };
    // biome-ignore lint/complexity/useArrowFunction: ResizeObserver is constructed with new.
    vi.spyOn(window, "ResizeObserver").mockImplementation(function (callback) {
      notifyResize = () => callback([], observer);
      return observer;
    });
    const { container, rerender, unmount } = render(
      <UiSegmentedControl options={options} value="all" onChange={vi.fn()} label="Media" />
    );
    const highlight = container.querySelector(".ui-segmented-highlight");
    expect(highlight).toHaveStyle({ transform: "translate(3px, 3px)", width: "70px", transition: "none" });
    rerender(<UiSegmentedControl options={options} value="video" onChange={vi.fn()} label="Media" />);
    expect(highlight).toHaveStyle({ transform: "translate(143px, 3px)" });
    expect(highlight).not.toHaveStyle({ transition: "none" });
    notifyResize();
    expect(highlight).not.toHaveStyle({ transition: "none" });
    optionWidth = 80;
    notifyResize();
    expect(highlight).toHaveStyle({ transform: "translate(163px, 3px)", width: "80px", transition: "none" });
    rerender(<UiSegmentedControl options={options} value="image" onChange={vi.fn()} label="Media" />);
    expect(highlight).not.toHaveStyle({ transition: "none" });
    rerender(<UiSegmentedControl options={options.map(option => ({ ...option, label: `Translated ${option.label}` }))}
      value="image" onChange={vi.fn()} label="Media" />);
    expect(highlight).toHaveStyle({ transition: "none" });
    observer.disconnect.mockClear();
    unmount();
    expect(observer.disconnect).toHaveBeenCalledOnce();
  });
});
