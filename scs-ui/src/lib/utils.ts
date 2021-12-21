import type { Subscriber, Readable } from "svelte/store";
import type { Response } from "./api";

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
  get(): Value | null;
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
    get(): Value | null {
      return stored;
    },
  };
}

type ApiStoreCallback<Value> = (value: Promise<Value>) => void;
type ApiStoreUpdate<Params extends any[]> = (...params: Params) => Promise<void>;
type ApiStore<Value, Params extends any[]> = Readable<Promise<Value>> & { update: ApiStoreUpdate<Params> };

/**
 * Wraps an API request function and converts it into a Svelte store with an
 * `update` function that can be used to make the API call.
 *
 * This store holds a Promise, which allows it to be used with Svelte's `#await`:
 * ```svelte
 * <script>
 *   import { stores } from "$lib/api";
 * </script>
 * {#await $store}
 *  <Spinner />
 * {:then value}
 *  <Use {value} />
 * {:catch error}
 *  <Error {error} />
 * {/await}
 * ```
 *
 * The initial value of the store is a never-resolving Promise. Once the store's
 * `update` function is called, and the request finishes, the Promise is replaced
 * with one that depends on the result of the request. It either:
 * - Always resolves to a value in case of success
 * - Always rejects with an error in case of failure
 *
 * Note that each subscriber gets a unique `Promise` object, so that they may
 * chain `then`/`catch`/`finally` callbacks without interfering with eachother.
 */
export function apiStore<Value, Params extends any[]>(
  request: (...params: Params) => Promise<Response<Value>>
): ApiStore<Value, Params> {
  // https://svelte.dev/docs#component-format-script-4-prefix-stores-with-$-to-access-their-values-store-contract
  // Svelte store contract:
  // 1. Stores have a `subscribe` method, which returns a function to `unsubscribe`.
  // 2. The callback passed to `subscribe` is immediately invoked with the store's current value.
  // 3. When the value changes, all subscribers' callbacks are synchronously invoked.

  let promise: () => Promise<Value> = () => new Promise(() => {});
  const subscriptions = new Set<ApiStoreCallback<Value>>();
  return {
    subscribe(callback: (value: Promise<Value>) => void) {
      callback(promise());
      subscriptions.add(callback);
      return () => subscriptions.delete(callback);
    },
    async update(...params: Params) {
      const result = await request(...params);
      if (result.type === "success") {
        promise = () => Promise.resolve(result.data);
      } else {
        promise = () => Promise.reject(result.message);
      }
      subscriptions.forEach((callback) => callback(promise()));
    },
  };
}
