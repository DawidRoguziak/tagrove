import { act, renderHook } from "@testing-library/react";
import type { ChangeEvent, KeyboardEvent } from "react";
import { describe, expect, it, vi } from "vitest";
import type { ActiveToken, TagSuggestion } from "../../types";
import { useSearchTagatorHandlers } from "../useSearchTagatorHandlers";

function createSuggestions(): TagSuggestion[] {
  return [
    {
      value: "cat",
      indices: [[0, 2]]
    },
    {
      value: "car",
      indices: [[0, 2]]
    }
  ];
}

function createActiveToken(): ActiveToken {
  return {
    start: 0,
    end: 2,
    query: "ca",
    negative: false
  };
}

describe("useSearchTagatorHandlers", () => {
  it("focuses input and updates caret with suggestions visibility", () => {
    const setInputFocused = vi.fn();
    const setSuggestionsOpen = vi.fn();
    const setCaretPosition = vi.fn();

    const { result } = renderHook(() =>
      useSearchTagatorHandlers({
        value: "cat",
        onValueChange: vi.fn(),
        onSubmit: vi.fn(),
        inputRef: undefined,
        keepSuggestionsOpenOnPick: false,
        activeToken: createActiveToken(),
        suggestions: createSuggestions(),
        suggestionsOpen: false,
        activeSuggestionIdx: 0,
        defaultActiveSuggestionIdx: 0,
        inputFocused: false,
        setInputFocused,
        setSuggestionsOpen,
        setActiveSuggestionIdx: vi.fn(),
        setCaretPosition
      })
    );

    const input = document.createElement("input");
    input.value = "cat";
    input.setSelectionRange(2, 2);

    act(() => {
      result.current.setInputNode(input);
      result.current.handleInputFocus();
    });

    expect(setInputFocused).toHaveBeenCalledWith(true);
    expect(setCaretPosition).toHaveBeenCalledWith(2);
    expect(setSuggestionsOpen).toHaveBeenCalledWith(true);
  });

  it("updates value and handles keyboard suggestion pick", () => {
    const onValueChange = vi.fn();
    const onSubmit = vi.fn();
    const setSuggestionsOpen = vi.fn();
    const setActiveSuggestionIdx = vi.fn();

    const { result } = renderHook(() =>
      useSearchTagatorHandlers({
        value: "ca",
        onValueChange,
        onSubmit,
        keepSuggestionsOpenOnPick: false,
        activeToken: createActiveToken(),
        suggestions: createSuggestions(),
        suggestionsOpen: true,
        activeSuggestionIdx: 0,
        defaultActiveSuggestionIdx: 0,
        inputFocused: true,
        setInputFocused: vi.fn(),
        setSuggestionsOpen,
        setActiveSuggestionIdx,
        setCaretPosition: vi.fn()
      })
    );

    const changeEvent = {
      target: {
        value: "car",
        selectionStart: 2
      }
    } as unknown as ChangeEvent<HTMLInputElement>;

    act(() => {
      result.current.handleInputChange(changeEvent);
    });
    expect(onValueChange).toHaveBeenCalledWith("car");

    const enterEvent = {
      key: "Enter",
      preventDefault: vi.fn()
    } as unknown as KeyboardEvent<HTMLInputElement>;

    act(() => {
      result.current.handleInputKeyDown(enterEvent);
    });

    expect(enterEvent.preventDefault).toHaveBeenCalled();
    expect(onValueChange).toHaveBeenCalledWith("cat");
    expect(setSuggestionsOpen).toHaveBeenCalledWith(false);
    expect(setActiveSuggestionIdx).toHaveBeenCalledWith(0);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("uses onSuggestionPick and keeps suggestions open on pick", () => {
    const onSuggestionPick = vi.fn();
    const setSuggestionsOpen = vi.fn();
    const setActiveSuggestionIdx = vi.fn();

    const { result } = renderHook(() =>
      useSearchTagatorHandlers({
        value: "ca",
        onValueChange: vi.fn(),
        onSuggestionPick,
        keepSuggestionsOpenOnPick: true,
        activeToken: createActiveToken(),
        suggestions: createSuggestions(),
        suggestionsOpen: true,
        activeSuggestionIdx: 1,
        defaultActiveSuggestionIdx: -1,
        inputFocused: true,
        setInputFocused: vi.fn(),
        setSuggestionsOpen,
        setActiveSuggestionIdx,
        setCaretPosition: vi.fn()
      })
    );

    act(() => {
      result.current.applySuggestion("car", 1);
    });

    expect(onSuggestionPick).toHaveBeenCalledWith("car");
    expect(setSuggestionsOpen).toHaveBeenCalledWith(true);
    expect(setActiveSuggestionIdx).toHaveBeenCalledWith(1);
  });
});
