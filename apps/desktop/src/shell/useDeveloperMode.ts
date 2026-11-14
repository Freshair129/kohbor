import { useState } from "react";
import { DEVELOPER_MODE_DEFAULT, readDeveloperMode, writeDeveloperMode } from "./developerMode";

export function useDeveloperMode() {
  const [enabled, setEnabled] = useState(() => {
    try { return readDeveloperMode(window.localStorage); } catch { return DEVELOPER_MODE_DEFAULT; }
  });
  return {
    enabled,
    change(value: boolean) {
      setEnabled(value);
      try { writeDeveloperMode(window.localStorage, value); } catch { /* The in-memory switch still works. */ }
    },
  };
}
