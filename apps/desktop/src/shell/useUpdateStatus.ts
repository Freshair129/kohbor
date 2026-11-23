import { useEffect, useRef, useSyncExternalStore } from "react";
import { updateCommand } from "../runtime";
import { createUpdateStatusStore, type UpdateStatusStore } from "./updateStatusStore";

export function useUpdateStatus() {
  const storeRef = useRef<UpdateStatusStore | null>(null);
  if (storeRef.current === null) storeRef.current = createUpdateStatusStore(updateCommand);
  const store = storeRef.current;
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);

  useEffect(() => {
    // Mount only reads status. Automatic checks require an explicit session-only opt-in.
    void store.mount();
    return () => store.unmount();
  }, [store]);

  return { ...snapshot, check: store.check, install: store.install, openReleasePage: store.openReleasePage };
}
