import { createEffect, createMemo, createSignal, onCleanup, untrack } from "solid-js";
import { listTags } from "../../../platform/tauri/tags";
import {
  createFallbackPage,
  dedupeKnownTags,
  mergeTagPages
} from "./tagListSelectionService";
import type { TagListPage } from "../../../platform/tauri/contracts/tags";
const PAGE_SIZE = 100;
export function createTagList(options: {
  open: () => boolean;
  tags: () => string[];
  query: () => string;
  list: () => HTMLDivElement | undefined;
}) {
  let generation = 0;
  let busy = false;
  let autoLoadBlocked = false;
  let page: TagListPage = { items: [], total: 0 };
  const [state, setState] = createSignal(page);
  const [loading, setLoading] = createSignal(false);
  const [more, setMore] = createSignal(false);
  const fallback = createMemo(() => (options.open() ? dedupeKnownTags(options.tags()) : []));
  const publish = (next: TagListPage) => {
    page = next;
    setState(next);
  };
  createEffect(
    () => ({ open: options.open(), query: options.query().trim(), tags: fallback() }),
    (request) => {
      const id = ++generation;
      autoLoadBlocked = false;
      busy = request.open;
      setLoading(request.open);
      setMore(false);
      const local = createFallbackPage(request.tags, request.query, 0, PAGE_SIZE);
      publish(local);
      if (!request.open) return;
      const list = untrack(options.list);
      if (list) list.scrollTop = 0;
      void listTags({ query: request.query, offset: 0, limit: PAGE_SIZE })
        .then((next) => {
          if (id === generation) publish(next);
        })
        .catch(() => {
          if (id === generation) publish(local);
        })
        .finally(() => {
          if (id === generation) {
            busy = false;
            setLoading(false);
          }
        });
      return () => {
        generation++;
        busy = false;
      };
    }
  );
  async function loadMore() {
    if (!options.open() || busy || page.items.length >= page.total) return;
    busy = true;
    setMore(true);
    const id = generation,
      offset = page.items.length;
    const local = createFallbackPage(fallback(), options.query().trim(), offset, PAGE_SIZE);
    try {
      const next = await listTags({ query: options.query().trim(), offset, limit: PAGE_SIZE });
      if (id === generation)
        publish({ items: mergeTagPages(page.items, next.items), total: next.total });
    } catch {
      if (id === generation)
        publish({
          items: mergeTagPages(page.items, local.items),
          total: Math.max(page.total, local.total)
        });
    } finally {
      if (id === generation) {
        autoLoadBlocked = page.items.length <= offset;
        busy = false;
        setMore(false);
      }
    }
  }
  createEffect(
    () => ({
      open: options.open(),
      loading: loading(),
      more: more(),
      page: state(),
      list: options.list()
    }),
    (value) => {
      if (
        value.open &&
        !value.loading &&
        !value.more &&
        !autoLoadBlocked &&
        value.page.items.length > 0 &&
        value.page.items.length < value.page.total &&
        value.list &&
        value.list.scrollHeight <= value.list.clientHeight + 120
      )
        void untrack(loadMore);
    }
  );
  onCleanup(() => {
    generation++;
  });
  return {
    get page() {
      return state();
    },
    get loading() {
      return loading();
    },
    get loadingMore() {
      return more();
    },
    loadMore
  };
}
