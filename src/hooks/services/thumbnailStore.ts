type Listener = () => void;

export class ThumbnailStore {
  private paths = new Map<number, string>();
  private rendering = new Set<number>();
  private versions = new Map<number, number>();
  private listeners = new Map<number, Set<Listener>>();

  subscribe = (assetId: number, listener: Listener) => {
    const listeners = this.listeners.get(assetId) ?? new Set<Listener>();
    listeners.add(listener);
    this.listeners.set(assetId, listeners);
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) this.listeners.delete(assetId);
    };
  };

  getVersion = (assetId: number) => this.versions.get(assetId) ?? 0;
  getPath = (assetId: number) => this.paths.get(assetId);
  isRendering = (assetId: number) => this.rendering.has(assetId);

  sync(paths: Record<number, string>) {
    const incomingIds = new Set(Object.keys(paths).map(Number));
    for (const assetId of this.paths.keys()) {
      if (!incomingIds.has(assetId)) {
        this.paths.delete(assetId);
        this.bump(assetId);
      }
    }
    for (const [assetIdText, path] of Object.entries(paths)) {
      const assetId = Number(assetIdText);
      if (this.paths.get(assetId) !== path) {
        this.paths.set(assetId, path);
        this.bump(assetId);
      }
    }
  }

  markRendering(assetIds: Iterable<number>, rendering: boolean) {
    for (const assetId of assetIds) {
      const changed = rendering ? !this.rendering.has(assetId) : this.rendering.has(assetId);
      if (!changed) continue;
      if (rendering) this.rendering.add(assetId);
      else this.rendering.delete(assetId);
      this.bump(assetId);
    }
  }

  clear() {
    const changed = new Set([...this.paths.keys(), ...this.rendering]);
    this.paths.clear();
    this.rendering.clear();
    for (const assetId of changed) this.bump(assetId);
  }

  private bump(assetId: number) {
    this.versions.set(assetId, (this.versions.get(assetId) ?? 0) + 1);
    for (const listener of this.listeners.get(assetId) ?? []) listener();
  }
}
