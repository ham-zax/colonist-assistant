export class InteractionRenderGate {
  private readonly locks = new Set<string>();
  private deferred = false;

  hold(key: string): void {
    this.locks.add(key);
  }

  release(key: string): void {
    this.locks.delete(key);
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
