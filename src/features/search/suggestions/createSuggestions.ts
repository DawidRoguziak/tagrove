import { createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import type { SuggestionClient } from "./SuggestionClient";
import type { ActiveToken, TagSuggestion } from "./types";
let editorId = 0;
const EMPTY: TagSuggestion[] = [];
export function createSuggestions(
  client: SuggestionClient,
  options: {
    tags: () => string[];
    token: () => ActiveToken | null;
    used: () => Set<string>;
    eligible: () => boolean;
    excluded: () => string[];
  }
) {
  const editor = `solid-editor-${++editorId}`;
  const [result, setResult] = createSignal<{
    key: string;
    tags: string[];
    items: TagSuggestion[];
  } | null>(null);
  const [failed, setFailed] = createSignal(false);
  const [retry, setRetry] = createSignal(0);
  const key = createMemo(() =>
    JSON.stringify([
      options.eligible(),
      options.token(),
      [...options.used()],
      options.excluded(),
      retry()
    ])
  );
  createEffect(
    () => ({
      key: key(),
      tags: options.tags(),
      token: options.token(),
      eligible: options.eligible(),
      used: [...options.used()],
      excluded: options.excluded()
    }),
    (request) => {
      let current = true;
      setFailed(false);
      const token = request.token;
      if (
        !request.eligible ||
        !token ||
        !token.query.trim() ||
        (!token.literal && !token.negative && /^(gn|tags)(:.*)?$/i.test(token.query))
      )
        return;
      void client
        .search(editor, request.tags, token, request.used, request.excluded)
        .then((items) => {
          if (current) setResult({ key: request.key, tags: request.tags, items });
        })
        .catch(() => {
          if (current) setFailed(true);
        });
      return () => {
        current = false;
        client.cancel(editor);
      };
    }
  );
  onCleanup(() => client.cancel(editor));
  return {
    get items() {
      const value = result();
      return value?.key === key() && value.tags === options.tags() && options.eligible()
        ? value.items
        : EMPTY;
    },
    get failed() {
      return failed();
    },
    retry() {
      client.retry();
      setRetry((value) => value + 1);
    }
  };
}
