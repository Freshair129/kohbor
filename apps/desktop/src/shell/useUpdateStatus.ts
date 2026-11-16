import { useEffect, useRef, useSyncExternalStore } from "react";
import { updateCommand } from "../runtime";
import { createUpdateStatusStore, type UpdateStatusStore } from "./updateStatusStore";

export function useUpdateStatus() {
  const storeRef = useRef<UpdateStatusStore | null>(null);
  if (storeRef.current === null) storeRef.current = createUpdateStatusStore(updateCommand);
  const store = storeRef.current;
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);

  useEffect(() => {
    // Only read current host status on mount. Checks and installation are user actions.
    void store.mount();
  }, [store]);

  return { ...snapshot, check: store.check, install: store.install, openReleasePage: store.openReleasePage };
}
