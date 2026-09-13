import { cleanup, render, screen, fireEvent, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { openUrl } from "@tauri-apps/plugin-opener";
import { version } from "../../../../../package.json";
import { repository, developerName } from "../../../../../packaging/flatpak/publisher.json";
import licenseText from "../../../../../LICENSE?raw";
import privacyText from "../../../../../PRIVACY.md?raw";
import { AboutSection } from "../AboutSection";
import { SettingsPanel } from "../../SettingsPanel";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.mocked(openUrl).mockReset();
});

describe("About settings", () => {
  it("shows build metadata and complete local documents without opening the browser", async () => {
    const { container } = render(<AboutSection />);
    expect(screen.getByTestId("about-version")).toHaveTextContent(version);
    expect(screen.getAllByText(developerName)).toHaveLength(2);
    expect(screen.getByText("GPL-3.0-or-later")).toBeInTheDocument();
    const documents = container.querySelectorAll("pre");
    expect(documents[0].textContent).toBe(licenseText);
    expect(documents[1].textContent).toBe(privacyText);
    for (const details of container.querySelectorAll("details")) {
      expect(details.open).toBe(false);
      await userEvent.click(details.querySelector("summary")!);
      expect(details.open).toBe(true);
    }
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("uses the system opener only after a click and shows a selectable address on failure", async () => {
    vi.mocked(openUrl).mockRejectedValueOnce(new Error("No browser"));
    render(<AboutSection />);
    await userEvent.click(screen.getByRole("button", { name: "Source code on GitHub" }));
    expect(openUrl).toHaveBeenCalledWith(repository);
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not open the browser");
    const address = screen.getByRole("textbox", { name: "Link address" });
    expect(address).toHaveValue(repository);
    expect(address).toHaveAttribute("readonly");
    await userEvent.click(address);
    expect(address).toHaveFocus();
    vi.mocked(openUrl).mockResolvedValueOnce();
    await userEvent.click(screen.getByRole("button", { name: "Contact on GitHub" }));
    expect(openUrl).toHaveBeenLastCalledWith(`${repository}/issues`);
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });

  it("places About last and moves navigation focus to its section", async () => {
    const scroll = vi.fn();
    render(<main><SettingsPanel fullView /></main>);
    const target = document.getElementById("settings-about");
    if (!target) throw new Error("Missing About section");
    target.scrollIntoView = scroll;
    const link = screen.getByRole("link", { name: "About" });
    expect(link.parentElement?.lastElementChild).toBe(link);
    expect(target.nextElementSibling).toBeNull();
    await userEvent.click(link);
    expect(target).toHaveFocus();
    expect(link).toHaveAttribute("aria-current", "location");
    expect(scroll).toHaveBeenCalledWith({ block: "start", behavior: "instant" });
    const main = screen.getByRole("main");
    Object.defineProperties(main, {
      scrollHeight: { value: 1600 },
      clientHeight: { value: 900 },
      scrollTop: { value: 700, configurable: true }
    });
    fireEvent.scroll(main);
    expect(link).toHaveAttribute("aria-current", "location");
    Object.defineProperty(main, "scrollTop", { value: 0 });
    vi.spyOn(target, "getBoundingClientRect").mockReturnValue({
      top: 1200, bottom: 1600, left: 0, right: 900, width: 900, height: 400, x: 0, y: 1200,
      toJSON: () => ({})
    });
    fireEvent.scroll(main);
    expect(link).not.toHaveAttribute("aria-current");
  });
});
