import { useEffect, useState } from "react";

export function usePageVisibility(): DocumentVisibilityState | null {
  const [visibility, setVisibility] = useState<DocumentVisibilityState | null>(() =>
    typeof document === "undefined" ? null : document.visibilityState);
  useEffect(() => {
    const update = () => setVisibility(document.visibilityState);
    update();
    document.addEventListener("visibilitychange", update);
    return () => document.removeEventListener("visibilitychange", update);
  }, []);
  return visibility;
}
