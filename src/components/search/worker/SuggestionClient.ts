import type { ActiveToken, TagSuggestion } from "../types";
import type { SuggestionResponse } from "./protocol";
type Job = {
  id: number;
  version: number;
  token: ActiveToken;
  usedTags: string[];
  excludedTags: string[];
  resolve: (items: TagSuggestion[]) => void;
  reject: (error: Error) => void;
};

/** One worker and one running search. Each editor retains only its latest request. */
export class SuggestionClient {
  private worker: Worker | null = null;
  private vocabulary: string[] | null = null;
  private version = 0;
  private sentVersion = -1;
  private nextId = 0;
  private pending = new Map<string, Job>();
  private active: Job | null = null;
  private failure: Error | null = null;
  constructor(
    private createWorker = () =>
      new Worker(new URL("./suggestions.worker.ts", import.meta.url), { type: "module" })
  ) {}

  search(
    editor: string,
    tags: string[],
    token: ActiveToken,
    usedTags: string[],
    excludedTags: string[] = []
  ) {
    if (tags !== this.vocabulary) {
      this.vocabulary = tags;
      this.version++;
    }
    this.cancel(editor);
    const promise = new Promise<TagSuggestion[]>((resolve, reject) => {
      this.pending.set(editor, {
        id: ++this.nextId,
        version: this.version,
        token,
        usedTags,
        excludedTags,
        resolve,
        reject
      });
    });
    this.pump();
    return promise;
  }
  cancel(editor: string) {
    this.pending.get(editor)?.resolve([]);
    this.pending.delete(editor);
  }
  retry() {
    this.failure = null;
  }
  dispose() {
    this.worker?.terminate();
    this.worker = null;
    this.sentVersion = -1;
    this.active?.resolve([]);
    this.active = null;
    for (const job of this.pending.values()) job.resolve([]);
    this.pending.clear();
  }
  private fail = () => {
    const error = new Error("Tag suggestions unavailable");
    this.failure = error;
    this.active?.reject(error);
    this.active = null;
    for (const job of this.pending.values()) job.reject(error);
    this.pending.clear();
    this.worker?.terminate();
    this.worker = null;
    this.sentVersion = -1;
  };
  private pump() {
    if (this.active || this.pending.size === 0) return;
    if (this.failure) {
      this.fail();
      return;
    }
    try {
      if (!this.worker) {
        this.worker = this.createWorker();
        this.worker.onerror = this.fail;
        this.worker.onmessageerror = this.fail;
        this.worker.onmessage = ({ data }: MessageEvent<SuggestionResponse>) => {
          const job = this.active;
          if (!job || job.id !== data.requestId) return;
          this.active = null;
          job.resolve(
            data.version === this.version && job.version === data.version ? data.suggestions : []
          );
          this.pump();
        };
      }
      const entry = this.pending.entries().next().value;
      if (!entry) return;
      const [editor, job] = entry;
      this.pending.delete(editor);
      if (job.version !== this.version) {
        job.resolve([]);
        this.pump();
        return;
      }
      if (this.sentVersion !== this.version) {
        this.worker.postMessage({
          type: "vocabulary",
          version: this.version,
          tags: this.vocabulary
        });
        this.sentVersion = this.version;
      }
      this.active = job;
      this.worker.postMessage({
        type: "search",
        version: job.version,
        requestId: job.id,
        token: job.token,
        usedTags: job.usedTags,
        excludedTags: job.excludedTags
      });
    } catch {
      this.fail();
    }
  }
}
