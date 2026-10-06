/** Cached immutable snapshots let React subscribe without owning desktop services. */
export class Observable<T> {
  private listeners = new Set<() => void>();
  constructor(public value: T) {}
  getSnapshot = () => this.value;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };
  update(patch: Partial<T>) {
    if (!Object.keys(patch).some(key => !Object.is(this.value[key as keyof T], patch[key as keyof T]))) return;
    this.value = { ...this.value, ...patch };
    this.listeners.forEach(listener => listener());
  }
}
