import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MediaGroupSetter } from "../MediaGroupSetter";
import { useLightboxModalHandlers } from "../hooks/useLightboxModalHandlers";

afterEach(cleanup);

function setup(initialOrder = "", pending = false) {
  const save = vi.fn();
  const change = vi.fn();
  function Editor() {
    const [order, setOrder] = useState(initialOrder);
    const { handleApplyMediaGroup } = useLightboxModalHandlers({
      selectedId: 1,
      mediaGroupKeyEditor: "group",
      mediaGroupOrderEditor: order,
      onSaveMediaGroup: save,
      onDeleteMedia: vi.fn(),
      onClose: vi.fn()
    });
    return <MediaGroupSetter pending={pending} groupKey="group" groupOrder={order}
      groupCopyConfirmed={false} canCopyMediaGroup={false} copyMediaGroupTitle="Copy group"
      onCopyMediaGroup={vi.fn()} onGroupKeyChange={vi.fn()}
      onGroupOrderChange={(value) => { change(value); setOrder(value); }}
      onApply={handleApplyMediaGroup} />;
  }
  render(<Editor />);
  const input = screen.getByRole("textbox", { name: /order/i });
  return { input, save, change, apply: screen.getByRole("button", { name: "Apply" }), user: userEvent.setup() };
}

const invalidOrders = ["0", "00", "-1", "1.5", "+2", "1e3", "9007199254740992", "12x3", " 2", "2 ", "2\n", "NaN", "Infinity"];

describe("MediaGroupSetter order", () => {
  it("is a numeric-hinted textbox without native increment controls", async () => {
    const { input, user } = setup("2");
    expect(input).toHaveAttribute("type", "text");
    expect(input).toHaveAttribute("inputmode", "numeric");
    expect(screen.queryByRole("spinbutton")).not.toBeInTheDocument();
    await user.click(input);
    await user.keyboard("{ArrowUp}{ArrowDown}");
    expect(input).toHaveValue("2");
  });

  it("saves typed positive integers and clears to null", async () => {
    const { input, apply, save, user } = setup();
    await user.type(input, "123");
    await user.click(apply);
    expect(save).toHaveBeenLastCalledWith({ key: "group", order: 123 });
    await user.clear(input);
    await user.click(apply);
    expect(save).toHaveBeenLastCalledWith({ key: "group", order: null });
  });

  it.each(["00012", "9007199254740991"])("accepts pasted %s and saves its numeric value", async (value) => {
    const { input, apply, save, user } = setup();
    await user.click(input);
    await user.paste(value);
    expect(input).toHaveValue(value);
    await user.click(apply);
    expect(save).toHaveBeenCalledWith({ key: "group", order: Number(value) });
  });

  it.each(invalidOrders)("rejects the entire pasted change %j", async (value) => {
    const { input, change, user } = setup("7");
    await user.click(input);
    await user.keyboard("{Control>}a{/Control}");
    await user.paste(value);
    expect(input).toHaveValue("7");
    expect(change).not.toHaveBeenCalled();
  });

  it("rejects invalid typed characters and zero", async () => {
    const { input, change, user } = setup();
    await user.type(input, "0-+.e x");
    expect(input).toHaveValue("");
    expect(change).not.toHaveBeenCalled();
    await user.type(input, "2");
    await user.type(input, ".e+-x");
    expect(input).toHaveValue("2");
    expect(change).toHaveBeenCalledTimes(1);
  });

  it.each(invalidOrders)("preserves invalid preloaded %j and disables Apply until edited", async (value) => {
    const { input, apply, save, change, user } = setup(value);
    expect(input).toHaveValue(value.replace(/\n/g, ""));
    expect(change).not.toHaveBeenCalled();
    expect(apply).toBeDisabled();
    await user.click(apply);
    expect(save).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: "3" } });
    expect(apply).toBeEnabled();
    await user.click(apply);
    expect(save).toHaveBeenCalledWith({ key: "group", order: 3 });
  });

  it("keeps Apply disabled while pending", () => {
    expect(setup("2", true).apply).toBeDisabled();
  });
});
