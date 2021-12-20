import type { Subscriber } from "svelte/store";

/**
 * Staggered callback
 *
 * Upon receiving a value, starts a timer.
 * If while ticking another value is received, the timer resets.
 * After timer reaches `delay`, it triggers the `callback` with the latest
 * `event` and stops the timer.
 */
export function stagger<Args extends any[], This>(
  callback: (this: This, ...args: Args) => void,
  delay: number = 250
): (...args: Args) => void {
  let args: Args;
  let timeout = 0;
  return function (this: This, ...newArgs: Args) {
    const context: This = this;
    args = newArgs;
    clearTimeout(timeout);
    timeout = setTimeout(() => callback.apply(context, args), delay) as any;
  };
}

/**
 * Generates a random string of `length`.
 */
export const nonce = (length = 32) =>
  [...crypto.getRandomValues(new Uint8Array(length / 2))].map((v) => v.toString(16).padStart(2, "0")).join("");

export type LocalStore<Value> = {
  subscribe(subscriber: Subscriber<Value | null>): () => void;
  set(value: Value | null): void;
};
/**
 * Svelte store backed by `localStorage`
 */
export function localStore<Value>(key: string, initial?: () => Value | null): LocalStore<Value> {
  const subscribers = new Set<Subscriber<Value | null>>();
  let stored: Value | null = key in localStorage ? JSON.parse(localStorage[key]) : null;
  if (!stored) {
    if (initial) localStorage[key] = JSON.stringify((stored = initial()));
    else delete localStorage[key];
  }

  return {
    subscribe(subscriber: Subscriber<Value | null>) {
      subscriber(stored);
      subscribers.add(subscriber);
      return () => subscribers.delete(subscriber);
    },
    set(value: Value | null) {
      localStorage[key] = JSON.stringify((stored = value));
      subscribers.forEach((c) => c(value));
    },
  };
}
