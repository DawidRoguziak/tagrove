import Fuse from "fuse.js";
import {
  createTagFuse,
  buildTagSuggestions
} from "../features/search/suggestions/suggestionService";
import type { SuggestionRequest, SuggestionResponse } from "../features/search/suggestions/protocol";
/** In-process transport for DOM tests; worker lifecycle is tested separately. */
export class SuggestionWorker {
  onmessage: ((event: MessageEvent<SuggestionResponse>) => void) | null = null;
  onerror: (() => void) | null = null;
  onmessageerror: (() => void) | null = null;
  private version = 0;
  private index = createTagFuse([], Fuse);
  private stopped = false;
  postMessage(data: SuggestionRequest) {
    queueMicrotask(() => {
      if (this.stopped) return;
      if (data.type === "vocabulary") {
        this.version = data.version;
        this.index = createTagFuse(data.tags, Fuse);
      } else {
        this.onmessage?.(
          new MessageEvent("message", {
            data: {
              version: this.version,
              requestId: data.requestId,
              suggestions: buildTagSuggestions({
                activeToken: data.token,
                usedTags: new Set(data.usedTags),
                excludedTags: new Set(data.excludedTags),
                fuse: this.index
              })
            }
          })
        );
      }
    });
  }
  terminate() {
    this.stopped = true;
  }
}
