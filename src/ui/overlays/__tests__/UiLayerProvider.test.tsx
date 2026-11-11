import { cleanup, render, screen } from "@solidjs/testing-library";
import { createSignal, flush, Show } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UiLayerProvider, createUiLayer } from "../UiLayerProvider";

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});
function tick(ms = 20) {
  vi.advanceTimersByTime(ms);
  flush();
}

// Mirrors App.tsx: settings and gallery are mutually exclusive views; the gallery
// registers a nonmodal bulk layer while app-lifetime bulk mode is on.
function SettingsPage(props: { onClose: () => void }) {
  let container: HTMLDivElement | undefined;
  createUiLayer({
    active: () => true,
    modal: false,
    container: () => container,
    onEscape: props.onClose,
    getRestoreFocus: () => document.getElementById("open-settings")
  });
  return (
    <div
      ref={(el) => {
        container = el;
      }}
    >
      <button type="button" onClick={props.onClose}>
        Back
      </button>
    </div>
  );
}
function BulkLayer(props: { enabled: () => boolean }) {
  let container: HTMLDivElement | undefined;
  createUiLayer({
    active: props.enabled,
    modal: false,
    container: () => container,
    onEscape: () => {}
  });
  return (
    <div
      ref={(el) => {
        container = el;
      }}
    >
      <button type="button">Tile</button>
    </div>
  );
}
function NewerModal() {
  let container: HTMLDivElement | undefined;
  const { layerId } = createUiLayer({
    active: () => true,
    modal: true,
    container: () => container,
    onEscape: () => {}
  });
  return (
    <div
      data-ui-layer={layerId}
      ref={(el) => {
        container = el;
      }}
    >
      <button type="button" data-initial-focus>
        Modal action
      </button>
    </div>
  );
}
function shell(bulk: boolean) {
  const [settings, setSettings] = createSignal(false);
  const [modal, setModal] = createSignal(false);
  const [bulkOn] = createSignal(bulk);
  render(() => (
    <UiLayerProvider>
      <button id="open-settings" type="button" onClick={() => setSettings(true)}>
        Open settings
      </button>
      <Show when={settings()} fallback={<BulkLayer enabled={bulkOn} />}>
        <SettingsPage onClose={() => setSettings(false)} />
      </Show>
      <Show when={modal()}>
        <NewerModal />
      </Show>
    </UiLayerProvider>
  ));
  const opener = screen.getByRole("button", { name: "Open settings" });
  opener.focus();
  setSettings(true);
  flush();
  tick();
  screen.getByRole("button", { name: "Back" }).focus();
  return { opener, setSettings, setModal };
}

describe("UiLayerProvider focus restore", () => {
  it.each([false, true])("restores the settings opener on close (bulk mode %s)", (bulk) => {
    const view = shell(bulk);
    view.setSettings(false);
    flush();
    expect(screen.queryByRole("button", { name: "Back" })).toBeNull();
    tick();
    expect(view.opener).toHaveFocus();
  });

  it("restores when a nonmodal layer registers before the settings layer releases", () => {
    const [settings, setSettings] = createSignal(true);
    const [bulk, setBulk] = createSignal(false);
    render(() => (
      <UiLayerProvider>
        <button id="open-settings" type="button">
          Open settings
        </button>
        <BulkLayer enabled={bulk} />
        <Show when={settings()}>
          <SettingsPage onClose={() => setSettings(false)} />
        </Show>
      </UiLayerProvider>
    ));
    tick();
    screen.getByRole("button", { name: "Back" }).focus();
    setBulk(true);
    flush();
    setSettings(false);
    flush();
    tick();
    expect(screen.getByRole("button", { name: "Open settings" })).toHaveFocus();
  });

  it("does not steal focus that stayed connected below a nonmodal layer", () => {
    const [settings, setSettings] = createSignal(true);
    const [bulk, setBulk] = createSignal(false);
    render(() => (
      <UiLayerProvider>
        <button id="open-settings" type="button">
          Open settings
        </button>
        <BulkLayer enabled={bulk} />
        <Show when={settings()}>
          <SettingsPage onClose={() => setSettings(false)} />
        </Show>
      </UiLayerProvider>
    ));
    tick();
    setBulk(true);
    flush();
    const tile = screen.getByRole("button", { name: "Tile" });
    tile.focus();
    setSettings(false);
    flush();
    tick();
    expect(tile).toHaveFocus();
  });

  it("skips the restore when a newer modal layer is on top", () => {
    const view = shell(true);
    view.setSettings(false);
    view.setModal(true);
    flush();
    tick();
    expect(screen.getByRole("button", { name: "Modal action" })).toHaveFocus();
    expect(view.opener).not.toHaveFocus();
  });

  it("does not restore a layer released beneath an open modal", () => {
    const view = shell(false);
    view.setModal(true);
    flush();
    tick();
    const action = screen.getByRole("button", { name: "Modal action" });
    expect(action).toHaveFocus();
    view.setSettings(false);
    flush();
    tick();
    expect(action).toHaveFocus();
  });
});
