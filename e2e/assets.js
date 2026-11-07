// Library listing for E2E assertions through the gallery's own query session
// (grid-performance stage 10 removed the `list_assets` OFFSET command).
//
// Returns `{ items, total }` with `items` in gallery order and shaped like
// `AssetDetails`: the summary fields plus `path`, `size_bytes`, `thumb_path` and `tags`.
//
// Starting a query from any owner supersedes a start that is still running, and the
// app then shows its load error. The helper therefore waits until the app has no
// `start_asset_query` in flight, uses its own owner, and retries the gallery if it
// still caught the app mid-start.

const OWNER = "e2e-assertions";
const PAGE_SIZE = 256;
const DETAIL_BATCH = 32;

/**
 * @param {{ offset?: number, limit?: number, tagsAnd?: string[], tagsNot?: string[],
 *   kind?: string | null, favoritesOnly?: boolean, metaFilter?: object | null }} [filters]
 */
export async function listAssetsForAssertions(filters = {}) {
  const request = {
    offset: Math.max(0, filters.offset ?? 0),
    limit: Math.max(1, filters.limit ?? 500),
    tagsAnd: filters.tagsAnd ?? [],
    tagsNot: filters.tagsNot ?? [],
    kind: ["image", "gif", "video"].includes(filters.kind) ? filters.kind : null,
    favoritesOnly: filters.favoritesOnly ?? false,
    metaFilter: filters.metaFilter ?? null
  };
  const slot = `assets-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  await browser.execute((args, key, owner, pageSize, detailBatch) => {
    // Track the app's own query starts through the IPC fetch bridge (once per page load).
    if (!window.e2eAssetsTracker) {
      const tracker = { starts: 0 };
      window.e2eAssetsTracker = tracker;
      const original = window.fetch;
      window.fetch = function (input, options) {
        const url = new URL(String(input), location.href);
        const start = url.protocol === "ipc:" && decodeURIComponent(url.pathname.slice(1)) === "start_asset_query";
        if (!start) return original.call(this, input, options);
        tracker.starts++;
        return original.call(this, input, options).finally(() => { tracker.starts--; });
      };
    }
    window.e2eAssets ??= {};
    window.e2eAssets[key] = { done: false };
    const invoke = (command, payload) => window.__TAURI_INTERNALS__.invoke(command, payload);
    const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
    const run = async () => {
      for (let attempt = 0; attempt < 5; attempt++) {
        for (let i = 0; i < 200 && window.e2eAssetsTracker.starts > 0; i++) await pause(25);
        // Generations must grow per owner for the backend's lifetime, across page reloads.
        window.e2eAssetsGeneration = Math.max((window.e2eAssetsGeneration ?? 0) + 1, Date.now());
        const start = await invoke("start_asset_query", {
          tagsAnd: args.tagsAnd, tagsNot: args.tagsNot, kind: args.kind, favoritesOnly: args.favoritesOnly,
          metaFilter: args.metaFilter, ownerId: owner, generation: window.e2eAssetsGeneration, pageSize, anchorOffset: null
        });
        if (start.status !== "ready") continue;
        const end = Math.min(start.total, args.offset + args.limit);
        const summaries = [];
        let stale = false;
        for (let offset = Math.floor(args.offset / pageSize) * pageSize; offset < end && !stale; offset += pageSize) {
          const page = offset === start.offset ? start
            : await invoke("get_asset_query_page", { sessionId: start.session_id, offset, limit: pageSize });
          if (page.status !== "ready") stale = true;
          else summaries.push(...page.items.filter((_, i) => offset + i >= args.offset && offset + i < end));
        }
        if (stale) continue;
        const items = [];
        for (let i = 0; i < summaries.length; i += detailBatch) {
          const batch = await Promise.all(summaries.slice(i, i + detailBatch)
            .map(summary => invoke("get_asset_details", { assetId: summary.id })));
          if (batch.some(details => !details)) { stale = true; break; }
          items.push(...batch);
        }
        if (!stale) return { items, total: start.total };
      }
      throw new Error("listAssetsForAssertions: query kept going stale or superseded");
    };
    run().then(
      value => { window.e2eAssets[key] = { done: true, value }; },
      error => { window.e2eAssets[key] = { done: true, error: String(error) }; }
    );
  }, request, slot, OWNER, PAGE_SIZE, DETAIL_BATCH);
  await browser.waitUntil(() => browser.execute(key => window.e2eAssets[key].done, slot), {
    timeout: 120000, interval: 50, timeoutMsg: "listAssetsForAssertions did not finish"
  });
  const response = await browser.execute(key => {
    const result = window.e2eAssets[key];
    delete window.e2eAssets[key];
    return result;
  }, slot);
  if (response.error) throw new Error(response.error);
  // If the app still had a start in flight, the gallery shows its load error: retry it.
  await browser.execute(() => {
    document.querySelector('[data-testid="gallery-load-error"] button')?.click();
  });
  return response.value;
}
