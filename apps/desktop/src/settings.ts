import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { Locale } from "./copy";
import type { UserSettingsView } from "./types";

export type SettingsNotice = "load_failed" | "conflict" | "refresh_failed" | "save_failed";

function requestId(): string {
  if (typeof window.crypto.randomUUID === "function") return window.crypto.randomUUID();
  return `settings-${Date.now()}-${Math.random().toString(36).slice(2, 12)}`;
}

function errorCode(error: unknown): string {
  if (typeof error === "string") return error.trim();
  if (error instanceof Error) return error.message.trim();
  return String(error).trim();
}

async function readSettings(): Promise<UserSettingsView> {
  return invoke<UserSettingsView>("settings_get");
}

export function useSettings() {
  const [settings, setSettings] = useState<UserSettingsView | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<SettingsNotice | null>(null);

  useEffect(() => {
    let alive = true;
    void readSettings()
      .then((value) => {
        if (!alive) return;
        setSettings(value);
        setNotice(null);
      })
      .catch(() => {
        if (alive) {
          setSettings(null);
          setNotice("load_failed");
        }
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  const changeLocale = useCallback(
    async (locale: Locale) => {
      if (!settings || saving || settings.locale === locale) return;
      const baseRevision = settings.revision;
      setSaving(true);
      setNotice(null);
      try {
        const updated = await invoke<UserSettingsView>("settings_patch", {
          request: {
            schema_version: "1.0.0",
            request_id: requestId(),
            base_revision: baseRevision,
            patch: { locale },
          },
        });
        setSettings(updated);
      } catch (error) {
        const conflict = errorCode(error) === "VERSION_UNSUPPORTED";
        try {
          const latest = await readSettings();
          setSettings(latest);
          setNotice(conflict && latest.revision !== baseRevision ? "conflict" : "save_failed");
        } catch {
          setSettings(null);
          setNotice("refresh_failed");
        }
      } finally {
        setSaving(false);
      }
    },
    [settings, saving],
  );

  return { settings, loading, saving, notice, changeLocale };
}
