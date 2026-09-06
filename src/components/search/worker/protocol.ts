import type { ActiveToken, TagSuggestion } from "../types";
export type SuggestionRequest =
  | { type: "vocabulary"; version: number; tags: string[] }
  | {
      type: "search";
      version: number;
      requestId: number;
      token: ActiveToken;
      usedTags: string[];
      excludedTags: string[];
    };
export interface SuggestionResponse {
  version: number;
  requestId: number;
  suggestions: TagSuggestion[];
}
