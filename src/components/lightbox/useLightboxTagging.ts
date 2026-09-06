import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isValidTag, normalizeTags } from "../../utils/media";

interface UseLightboxTaggingOptions {
  selectedId: number | null;
  tagEditor: string[];
  onTagEditorChange: (value: string[]) => void;
  onSaveTags: (tags?: string[]) => void | Promise<void>;
  onRetryTags: () => void;
  tagSaving: boolean;
  tagFailed: boolean;
  tagDetailsLoading: boolean;
  tagDetailsFailed: boolean;
  knownTags: string[];
}

export function useLightboxTagging({
  selectedId,
  tagEditor,
  onTagEditorChange,
  onSaveTags,
  onRetryTags,
  tagSaving,
  tagFailed,
  tagDetailsLoading,
  tagDetailsFailed,
  knownTags
}: UseLightboxTaggingOptions) {
  const tagInputRef = useRef<HTMLInputElement | null>(null);
  const [tagDraft, setTagDraft] = useState("");
  const tagEditingDisabled = tagDetailsLoading || tagDetailsFailed;

  const selectedTags = useMemo(() => normalizeTags(tagEditor), [tagEditor]);
  const selectedTagSet = useMemo(() => new Set(selectedTags), [selectedTags]);
  const availableKnownTags = knownTags;

  // biome-ignore lint/correctness/useExhaustiveDependencies: selected identity resets only the local input draft.
  useEffect(() => {
    setTagDraft("");
  }, [selectedId]);

  const applyTags = useCallback((nextTags: string[]) => {
    if (tagEditingDisabled) return;
    const normalized = normalizeTags(nextTags);
    onTagEditorChange(normalized);
    void Promise.resolve(onSaveTags(normalized)).catch(() => {});
  }, [onSaveTags, onTagEditorChange, tagEditingDisabled]);

  const addTag = useCallback((rawValue: string) => {
    if (tagEditingDisabled) return;
    const nextTag = rawValue.trim().toLowerCase();
    if (!isValidTag(nextTag) || selectedTagSet.has(nextTag)) return;
    applyTags([...selectedTags, nextTag]);
    setTagDraft("");
    requestAnimationFrame(() => tagInputRef.current?.focus());
  }, [applyTags, selectedTagSet, selectedTags, tagEditingDisabled]);

  const removeTag = useCallback((tag: string) => {
    if (tagEditingDisabled) return;
    applyTags(selectedTags.filter((item) => item !== tag));
  }, [applyTags, selectedTags, tagEditingDisabled]);

  return {
    tagInputRef,
    selectedTags,
    tagDraft,
    tagSaving,
    tagFailed,
    tagEditingDisabled,
    availableKnownTags,
    addTag,
    removeTag,
    retryTags: onRetryTags,
    setTagDraft
  };
}
