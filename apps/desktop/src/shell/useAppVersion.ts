import { useEffect, useState } from "react";
import { appVersion } from "../runtime";
import { readAppVersion, type AppVersionInfo } from "./appVersionModel";

export function useAppVersion(): AppVersionInfo | null {
  const [info, setInfo] = useState<AppVersionInfo | null>(null);
  useEffect(() => {
    let disposed = false;
    void readAppVersion(appVersion).then(value => { if (!disposed) setInfo(value); });
    return () => { disposed = true; };
  }, []);
  return info;
}
