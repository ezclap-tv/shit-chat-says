import type { Readable } from "svelte/store";
import api, { type Response } from "./index";

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
function apiStore<Value, Params extends any[]>(
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

export const channels = apiStore(api.user.logs.channels);

// @ts-ignore
globalThis._stores = { channels };
