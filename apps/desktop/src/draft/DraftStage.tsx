import { useEffect, useState, type CSSProperties } from "react";
import { draftColumnApi, draftHostAvailable, subscribeDraftView } from "./draftColumnApi";
import { DraftColumnView } from "./DraftColumnView";
import {
  draftColumnPlacement,
  HIDDEN_DRAFT_COLUMN,
  type DraftColumnView as DraftColumnViewModel,
} from "./draftColumnModel";

function useColumnPlacement() {
  const [placement, setPlacement] = useState(() => draftColumnPlacement(window.innerWidth, window.innerHeight));
  useEffect(() => {
    const resize = () => setPlacement(draftColumnPlacement(window.innerWidth, window.innerHeight));
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  return placement;
}

export function DraftStage() {
  const [view, setView] = useState<DraftColumnViewModel>(HIDDEN_DRAFT_COLUMN);
  const placement = useColumnPlacement();
  const synthetic = import.meta.env.DEV && new URLSearchParams(window.location.search).get("mock") === "1";

  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    if (!draftHostAvailable()) return;
    if (synthetic) {
      void draftColumnApi.getView().then((next) => {
        if (!disposed) setView(next);
      }).catch(() => {
        if (!disposed) setView(HIDDEN_DRAFT_COLUMN);
      });
      return () => { disposed = true; };
    }
    void subscribeDraftView((next) => {
      if (!disposed) setView(next);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    }).catch(() => {
      if (!disposed) setView(HIDDEN_DRAFT_COLUMN);
    });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [synthetic]);

  return (
    <main className="draft-stage" lang="th" aria-live="off" style={{
      "--draft-overlay-scale": placement?.scale ?? 0,
      "--draft-overlay-x": `${placement?.x ?? 0}px`,
      "--draft-overlay-y": `${placement?.y ?? 0}px`,
    } as CSSProperties}>
      <DraftColumnView view={view} surface="overlay" synthetic={synthetic} />
    </main>
  );
}
