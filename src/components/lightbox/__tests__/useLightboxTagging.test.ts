import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useLightboxTagging } from "../useLightboxTagging";

function options(overrides: Partial<Parameters<typeof useLightboxTagging>[0]> = {}) {
  return {
    selectedId: 1,
    tagEditor: ["cat"],
    onTagEditorChange: vi.fn(),
    onSaveTags: vi.fn(),
    onRetryTags: vi.fn(),
    tagSaving: false,
    tagFailed: false,
    tagDetailsLoading: false,
    tagDetailsFailed: false,
    knownTags: ["cat", "dog"],
    tagsPanelOpen: true,
    ...overrides
  };
}

describe("useLightboxTagging", () => {
  it("normalizes additions and delegates complete replacements to the owner", () => {
    const onTagEditorChange = vi.fn();
    const onSaveTags = vi.fn();
    const { result } = renderHook(() => useLightboxTagging(options({
      onTagEditorChange,
      onSaveTags
    })));

    act(() => result.current.addTag(" ŻÓŁW "));

    expect(onTagEditorChange).toHaveBeenCalledWith(["cat", "żółw"]);
    expect(onSaveTags).toHaveBeenCalledWith(["cat", "żółw"]);
  });

  it("rejects tags that cannot round-trip through CSV and search", () => {
    const onTagEditorChange = vi.fn();
    const onSaveTags = vi.fn();
    const { result } = renderHook(() => useLightboxTagging(options({
      onTagEditorChange,
      onSaveTags
    })));

    act(() => result.current.addTag("New York"));
    act(() => result.current.addTag("cat,dog"));

    expect(onTagEditorChange).not.toHaveBeenCalled();
    expect(onSaveTags).not.toHaveBeenCalled();
  });

  it("delegates removal and retry without coordinating persistence locally", () => {
    const onSaveTags = vi.fn();
    const onRetryTags = vi.fn();
    const { result } = renderHook(() => useLightboxTagging(options({ onSaveTags, onRetryTags })));

    act(() => result.current.removeTag("cat"));
    act(() => result.current.retryTags());

    expect(onSaveTags).toHaveBeenCalledWith([]);
    expect(onRetryTags).toHaveBeenCalledTimes(1);
  });

  it("blocks additions and removals while the authoritative tag base is unavailable", () => {
    const onTagEditorChange = vi.fn();
    const onSaveTags = vi.fn();
    const { result } = renderHook(() => useLightboxTagging(options({
      onTagEditorChange,
      onSaveTags,
      tagDetailsFailed: true
    })));

    act(() => result.current.addTag("dog"));
    act(() => result.current.removeTag("cat"));

    expect(result.current.tagEditingDisabled).toBe(true);
    expect(onTagEditorChange).not.toHaveBeenCalled();
    expect(onSaveTags).not.toHaveBeenCalled();
  });
});
