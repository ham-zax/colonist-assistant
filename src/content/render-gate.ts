export class InteractionRenderGate {
  private readonly locks = new Set<string>();
  private deferred = false;

  hold(key: string): void {
    this.locks.add(key);
  }

  release(key: string): boolean {
    this.locks.delete(key);
    if (this.locks.size > 0 || !this.deferred) return false;
    this.deferred = false;
    return true;
  }

  tryRender(): boolean {
    if (this.locks.size > 0) {
      this.deferred = true;
      return false;
    }
    this.deferred = false;
    return true;
  }

  hasDeferredRender(): boolean {
    return this.deferred;
  }
}
