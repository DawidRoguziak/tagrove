import Fuse from "fuse.js";
import { buildTagSuggestions, createTagFuse } from "../services/suggestionService";
import type { SuggestionRequest, SuggestionResponse } from "./protocol";
let version = 0;
let index = createTagFuse([], Fuse);
self.onmessage = ({ data }: MessageEvent<SuggestionRequest>) => {
  if (data.type === "vocabulary") {
    version = data.version;
    index = createTagFuse(data.tags, Fuse);
    return;
  }
  const response: SuggestionResponse = {
    version,
    requestId: data.requestId,
    suggestions:
      data.version === version
        ? buildTagSuggestions({
            activeToken: data.token,
            usedTags: new Set(data.usedTags),
            excludedTags: new Set(data.excludedTags),
            fuse: index
          })
        : []
  };
  self.postMessage(response);
};
