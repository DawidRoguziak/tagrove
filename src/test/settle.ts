import { flush } from "solid-js";

/** Explicit Solid scheduling boundary for model tests, including deferred work. */
export function settle<T>(operation: () => Promise<T>): Promise<T>;
export function settle<T>(operation: () => T): T;
export function settle<T>(operation: () => T | Promise<T>): T | Promise<T> {
  const result = operation();
  if (result instanceof Promise)
    return result.finally(() => {
      flush();
    });
  flush();
  return result;
}
