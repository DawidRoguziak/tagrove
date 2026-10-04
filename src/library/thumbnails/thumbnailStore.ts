type Listener = () => void;

export class ThumbnailStore {
  private retained: Set<number> | null = null;
  private paths = new Map<number, string>();
  private rendering = new Set<number>();
  private nextVersion = 0;
  private versions = new Map<number, number>();
  private listeners = new Map<number, Set<Listener>>();

  subscribe = (assetId: number, listener: Listener) => {
    const listeners = this.listeners.get(assetId) ?? new Set<Listener>();
    listeners.add(listener);
    this.listeners.set(assetId, listeners);
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) {
        this.listeners.delete(assetId);
        if (this.retained && !this.retained.has(assetId)) this.paths.delete(assetId);
        this.releaseVersion(assetId);
      }
    };
  };

  getVersion = (assetId: number) => this.versions.get(assetId) ?? 0;
  getPath = (assetId: number) => this.paths.get(assetId);
  isRendering = (assetId: number) => this.rendering.has(assetId);

  retain(ids: Iterable<number>) {
    this.retained = new Set(ids);
    for (const id of this.paths.keys()) {
      if (!this.retained.has(id) && !this.listeners.has(id)) { this.paths.delete(id); this.bump(id); }
    }
  }

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

  complete(paths: Record<number, string>, finishedIds: Iterable<number>) {
    const changed = new Set<number>();
    for (const [idText, path] of Object.entries(paths)) {
      const id = Number(idText);
      if (this.retained && !this.retained.has(id) && !this.listeners.has(id)) continue;
      if (this.paths.get(id) !== path) {
        this.paths.set(id, path);
        changed.add(id);
      }
    }
    for (const id of finishedIds) {
      if (this.rendering.delete(id)) changed.add(id);
    }
    for (const id of changed) this.bump(id);
  }

  private releaseVersion(assetId: number) {
    if (!this.paths.has(assetId) && !this.rendering.has(assetId) && !this.listeners.has(assetId)) {
      this.versions.delete(assetId);
    }
  }

  clear() {
    const changed = new Set([...this.paths.keys(), ...this.rendering]);
    this.paths.clear();
    this.rendering.clear();
    for (const assetId of changed) this.bump(assetId);
  }

  private bump(assetId: number) {
    this.versions.set(assetId, ++this.nextVersion);
    for (const listener of this.listeners.get(assetId) ?? []) listener();
    this.releaseVersion(assetId);
  }
}
