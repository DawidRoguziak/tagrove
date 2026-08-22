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
  tagsPanelOpen: boolean;
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
  knownTags,
  tagsPanelOpen
}: UseLightboxTaggingOptions) {
  const tagInputRef = useRef<HTMLInputElement | null>(null);
  const [tagDraft, setTagDraft] = useState("");
  const tagEditingDisabled = tagDetailsLoading || tagDetailsFailed;

  const selectedTags = normalizeTags(tagEditor);
  const selectedTagSet = useMemo(() => new Set(selectedTags), [selectedTags]);
  const availableKnownTags = useMemo(
    () => knownTags.filter((tag) => !selectedTagSet.has(tag.toLowerCase())),
    [knownTags, selectedTagSet]
  );

  useEffect(() => {
    setTagDraft("");
  }, [selectedId]);

  const applyTags = useCallback((nextTags: string[]) => {
    if (tagEditingDisabled) return;
    const normalized = normalizeTags(nextTags);
    onTagEditorChange(normalized);
    void onSaveTags(normalized);
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

  useEffect(() => {
    if (!tagsPanelOpen || tagEditingDisabled) return;
    const frame = window.requestAnimationFrame(() => tagInputRef.current?.focus());
    return () => window.cancelAnimationFrame(frame);
  }, [tagEditingDisabled, tagsPanelOpen]);

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
