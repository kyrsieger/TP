import { useSyncExternalStore } from "react";
import type { Observable } from "../state/observable";

export function useObservable<T>(store: Observable<T>) {
  return useSyncExternalStore(store.subscribe, store.getSnapshot);
}
