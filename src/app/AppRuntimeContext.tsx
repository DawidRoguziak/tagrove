import { createContext, useContext } from "solid-js";
import type { JSX } from "@solidjs/web";
import { createAppRuntime } from "./createAppRuntime";
const RuntimeContext = createContext<ReturnType<typeof createAppRuntime>>();
export function AppRuntimeProvider(props: { children: JSX.Element }) {
  const runtime = createAppRuntime();
  return <RuntimeContext value={runtime}>{props.children}</RuntimeContext>;
}
export function useAppRuntime() {
  const runtime = useContext(RuntimeContext);
  if (!runtime) throw new Error("AppRuntimeProvider is required");
  return runtime;
}
