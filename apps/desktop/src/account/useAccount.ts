import { useCallback, useSyncExternalStore } from "react";
import { guestAccountPort, type AccountPort } from "./accountView";

export function useAccount(port: AccountPort = guestAccountPort) {
  const subscribe = useCallback((cb: () => void) => port.subscribe(cb), [port]);
  const get = useCallback(() => port.get(), [port]);
  return useSyncExternalStore(subscribe, get, get);
}

export function useAccountSnapshot(port: AccountPort = guestAccountPort) {
  const subscribe = useCallback((cb: () => void) => port.subscribe(cb), [port]);
  const get = useCallback(() => port.getState(), [port]);
  return useSyncExternalStore(subscribe, get, get);
}
