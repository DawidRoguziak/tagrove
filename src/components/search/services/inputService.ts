import type { ActiveToken } from "../types";

interface AppliedSuggestion {
  nextValue: string;
  nextCaret: number;
}

export function applySuggestionToValue(
  value: string,
  activeToken: ActiveToken,
  tag: string
): AppliedSuggestion {
  const insertedTag = `${activeToken.negative ? "-" : ""}${tag}`;
  const nextValue = value.slice(0, activeToken.start) + insertedTag + value.slice(activeToken.end);
  const nextCaret = activeToken.start + insertedTag.length;

  return {
    nextValue,
    nextCaret
  };
}
