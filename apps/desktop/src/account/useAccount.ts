import { useCallback, useSyncExternalStore } from "react";
import type { AccountPort } from "./accountView";
export function useAccount(port: AccountPort) {
  const subscribe = useCallback((cb: () => void) => port.subscribe(cb), [port]);
  const get = useCallback(() => port.get(), [port]);
  return useSyncExternalStore(subscribe, get, get);
}
