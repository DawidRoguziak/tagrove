import { fireEvent, render, screen, within } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { UiButton } from "../UiButton";
import { UiIcon } from "../UiIcon";
import { UiIconButton } from "../UiIconButton";
import { UiChip } from "../UiChip";
import { UiAlert } from "../UiAlert";
import { UiProgressBar } from "../UiProgressBar";
import { UiSlider } from "../UiSlider";
import { UiSegmentedControl } from "../UiSegmentedControl";
const options = [
  { value: "all", label: "All" },
  { value: "image", label: "Images" },
  { value: "video", label: "Video" }
] as const;
type Choice = (typeof options)[number]["value"];
afterEach(() => vi.restoreAllMocks());
describe("Solid shared controls", () => {
  it("updates attributes, styling, icon shape and accessible names on the same nodes", () => {
    const [active, setActive] = createSignal(false);
    const clicked = vi.fn();
    render(() => (
      <>
        <UiButton variant={active() ? "danger" : "primary"} disabled={active()} onClick={clicked}>
          Action
        </UiButton>
        <UiIconButton
          icon={active() ? "heart" : "settings"}
          active={active()}
          aria-label="Icon action"
        />
        <UiChip tone={active() ? "video" : "gif"}>Kind</UiChip>
        <UiAlert tone={active() ? "error" : "info"} title={active() ? "Failed" : "Info"}>
          Message
        </UiAlert>
        <UiProgressBar value={active() ? 75 : 25} max={100} label="Progress" />
      </>
    ));
    const button = screen.getByRole("button", { name: "Action" });
    expect(button).toHaveAttribute("type", "button");
    fireEvent.click(button);
    expect(clicked).toHaveBeenCalledOnce();
    const svg = screen.getByRole("button", { name: "Icon action" }).querySelector("svg");
    const oldPath = svg?.innerHTML;
    setActive(true);
    flush();
    expect(button).toBeDisabled();
    expect(button).toHaveClass("text-error");
    expect(svg?.innerHTML).not.toBe(oldPath);
    expect(svg).toHaveClass("fill-current");
    expect(svg).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByText("Kind")).toHaveClass("bg-[var(--media-video-bg)]");
    expect(screen.getByRole("alert")).toHaveTextContent("Failed");
    expect(screen.getByRole("alert")).toHaveClass("bg-error/14");
    expect(screen.getByRole("progressbar")).toHaveAttribute("value", "75");
  });
  it("preserves warning alert styling and content", () => {
    render(() => (
      <UiAlert tone="warning" title="Overwrite warning">
        Existing values will be replaced.
      </UiAlert>
    ));
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Overwrite warning");
    expect(alert).toHaveTextContent("Existing values will be replaced.");
    expect(alert).toHaveClass("border-warning/52", "text-base-content");
  });

  it("gives each titled icon its own SVG children and label id", () => {
    render(() => (
      <>
        <UiIcon name="search" title="First" />
        <UiIcon name="search" title="Second" />
      </>
    ));
    const first = screen.getByRole("img", { name: "First" }),
      second = screen.getByRole("img", { name: "Second" });
    expect(first.querySelectorAll("path,circle")).toHaveLength(2);
    expect(second.querySelectorAll("path,circle")).toHaveLength(2);
    expect(first.getAttribute("aria-labelledby")).not.toBe(second.getAttribute("aria-labelledby"));
  });
  it("updates a slider through native input and external value changes", () => {
    const [value, setValue] = createSignal(20);
    render(() => (
      <UiSlider
        min={0}
        max={100}
        value={value()}
        aria-label="Zoom"
        onInput={(event) => setValue(Number(event.currentTarget.value))}
      />
    ));
    const slider = screen.getByRole("slider");
    fireEvent.input(slider, { target: { value: "40" } });
    flush();
    expect(slider).toHaveValue("40");
    setValue(80);
    flush();
    expect(slider).toHaveValue("80");
  });
  it("keeps the committed slider value when its parent rejects an input", () => {
    const input = vi.fn();
    render(() => <UiSlider min={0} max={100} value={20} aria-label="Zoom" onInput={input} />);
    const slider = screen.getByRole("slider");
    fireEvent.input(slider, { target: { value: "80" } });
    flush();
    expect(input).toHaveBeenCalledOnce();
    expect(slider).toHaveValue("20");
  });

  it("names radio groups and waits for controlled selection", async () => {
    const [value, setValue] = createSignal<Choice>("all");
    const change = vi.fn();
    render(() => (
      <UiSegmentedControl
        options={options}
        value={value()}
        onChange={change}
        label="Media type"
        id="media"
        className="toolbar-control"
      />
    ));
    const group = screen.getByRole("group", { name: "Media type" });
    expect(group).toHaveAttribute("id", "media");
    expect(group).toHaveClass("toolbar-control");
    expect(within(group).getAllByRole("radio")).toHaveLength(3);
    for (const option of options)
      expect(within(group).getByRole("radio", { name: option.label })).toHaveAccessibleName(
        option.label
      );
    await userEvent.click(screen.getByRole("radio", { name: "Video" }));
    expect(change).toHaveBeenCalledExactlyOnceWith("video");
    expect(screen.getByRole("radio", { name: "All" })).toBeChecked();
    setValue("video");
    flush();
    expect(screen.getByRole("radio", { name: "Video" })).toBeChecked();
    setValue("image");
    flush();
    expect(screen.getByRole("radio", { name: "Images" })).toBeChecked();
    expect(change).toHaveBeenCalledOnce();
  });
  it("keeps groups independent with native keyboard selection", async () => {
    function Control(props: { label: string }) {
      const [value, setValue] = createSignal<Choice>("all");
      return (
        <UiSegmentedControl
          options={options}
          value={value()}
          onChange={setValue}
          label={props.label}
        />
      );
    }
    render(() => (
      <>
        <Control label="First" />
        <Control label="Second" />
      </>
    ));
    const first = within(screen.getByRole("group", { name: "First" })),
      second = within(screen.getByRole("group", { name: "Second" }));
    expect(first.getByRole("radio", { name: "All" }).getAttribute("name")).not.toBe(
      second.getByRole("radio", { name: "All" }).getAttribute("name")
    );
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
  it("slides selection but snaps changed layout and translation, disposing observers", () => {
    let width = 70;
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
      this: HTMLElement
    ) {
      const index = options.findIndex(
        (option) => this.querySelector("input")?.value === option.value
      );
      return this.tagName === "LABEL"
        ? new DOMRect(index * width + 3, 3, width, 26)
        : new DOMRect(0, 0, 250, 32);
    });
    let resize = () => {};
    const observer = { observe: vi.fn(), unobserve: vi.fn(), disconnect: vi.fn() };
    // biome-ignore lint/complexity/useArrowFunction: ResizeObserver is constructed with new.
    vi.spyOn(window, "ResizeObserver").mockImplementation(function (callback) {
      resize = () => callback([], observer);
      return observer;
    });
    const [value, setValue] = createSignal<Choice>("all");
    const [translated, setTranslated] = createSignal(false);
    const view = render(() => (
      <UiSegmentedControl
        options={
          translated() ? options.map((o) => ({ ...o, label: `Translated ${o.label}` })) : options
        }
        value={value()}
        onChange={() => {}}
        label="Media"
      />
    ));
    const highlight = view.container.querySelector(".ui-segmented-highlight");
    expect(highlight).toHaveStyle({
      transform: "translate(3px, 3px)",
      width: "70px",
      transition: "none"
    });
    setValue("video");
    flush();
    expect(highlight).toHaveStyle({ transform: "translate(143px, 3px)" });
    expect(highlight).not.toHaveStyle({ transition: "none" });
    resize();
    expect(highlight).not.toHaveStyle({ transition: "none" });
    width = 80;
    resize();
    expect(highlight).toHaveStyle({
      transform: "translate(163px, 3px)",
      width: "80px",
      transition: "none"
    });
    setValue("image");
    flush();
    expect(highlight).not.toHaveStyle({ transition: "none" });
    setTranslated(true);
    flush();
    expect(highlight).toHaveStyle({ transition: "none" });
    observer.disconnect.mockClear();
    view.unmount();
    expect(observer.disconnect).toHaveBeenCalledOnce();
  });
});
