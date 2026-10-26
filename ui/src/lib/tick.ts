// The playback state streamed from Rust about 30 times a second. It is kept out of React state
// so a tick does not re-render the whole app: components subscribe to just the part they show
// (useTick with a selector that returns a plain value), and the oscilloscope and the meters read
// it from animation frames without rendering at all.

import { useSyncExternalStore } from "react";
import type { Tick } from "./types";

let latest: Tick | null = null;
const listeners = new Set<() => void>();

export const tickStore = {
  get: (): Tick | null => latest,
  set(tick: Tick) {
    latest = tick;
    for (const f of listeners) f();
  },
  subscribe(f: () => void): () => void {
    listeners.add(f);
    return () => listeners.delete(f);
  },
};

/** A value picked from the latest tick; the component re-renders only when it changes. `select`
 * must return a plain value (number, string, boolean, null), not a new object. */
export function useTick<T>(select: (tick: Tick | null) => T): T {
  return useSyncExternalStore(tickStore.subscribe, () => select(latest));
}
