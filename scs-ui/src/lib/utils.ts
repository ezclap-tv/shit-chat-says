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
