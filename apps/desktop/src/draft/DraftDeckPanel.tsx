import { useEffect, useState } from "react";
import { draftColumnApi, draftHostAvailable, commandErrorText } from "./draftColumnApi";
import { DraftColumnView } from "./DraftColumnView";
import type { DraftColumnView as DraftColumnViewModel } from "./draftColumnModel";

const REFRESH_MS = 2000;

export function DraftDeckPanel({ visible = true }: { visible?: boolean }) {
  const [view, setView] = useState<DraftColumnViewModel | null>(null);
  const [error, setError] = useState<string | null>(null);
  const synthetic = import.meta.env.DEV && new URLSearchParams(window.location.search).get("mock") === "1";

  useEffect(() => {
    if (!visible || !draftHostAvailable()) return;
    let disposed = false;
    let timer: number | undefined;
    let requestInFlight = false;

    const poll = async () => {
      if (disposed || document.visibilityState !== "visible" || requestInFlight) return;
      requestInFlight = true;
      try {
        const next = await draftColumnApi.getView();
        if (!disposed && document.visibilityState === "visible") {
          setView(next);
          setError(null);
        }
      } catch (failure) {
        if (!disposed && document.visibilityState === "visible") setError(commandErrorText(failure));
      } finally {
        requestInFlight = false;
      }
    };

    const visibilityChanged = () => {
      if (document.visibilityState === "visible") {
        if (timer === undefined) {
          void poll();
          timer = window.setInterval(() => { void poll(); }, REFRESH_MS);
        }
      } else if (timer !== undefined) {
        window.clearInterval(timer);
        timer = undefined;
      }
    };

    visibilityChanged();
    document.addEventListener("visibilitychange", visibilityChanged);
    return () => {
      disposed = true;
      if (timer !== undefined) window.clearInterval(timer);
      document.removeEventListener("visibilitychange", visibilityChanged);
    };
  }, [visible]);

  return (
    <section className="deck-draft-column-panel" aria-label="สถิติฮีโร่สาธารณะ">
      {view
        ? <DraftColumnView view={view} surface="deck" synthetic={synthetic} />
        : <p className="draft-panel-loading" role="status">กำลังอ่านมุมมองสถิติฮีโร่…</p>}
      {error && <p className="draft-panel-error" role="alert">{error}</p>}
    </section>
  );
}
