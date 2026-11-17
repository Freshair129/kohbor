export interface AudioPrefs {
  master_volume_percent: number;
  hotkeys_enabled: boolean;
  hotkeys_registered: boolean | null;
  muted: boolean | null;
}

export interface AudioVolumeEvent {
  percent: number;
  source: "ui" | "hotkey";
}

const PREFS_KEYS = ["master_volume_percent", "hotkeys_enabled", "hotkeys_registered", "muted"] as const;
const VOLUME_EVENT_KEYS = ["percent", "source"] as const;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value);
  return actual.length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}

export function isValidVolumePercent(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= 100;
}

/** Parse the whole host contract. Any missing, extra, mistyped, or out-of-range field stays unknown. */
export function parseAudioPrefs(value: unknown): AudioPrefs | null {
  if (!isRecord(value) || !hasExactKeys(value, PREFS_KEYS)) return null;
  if (!isValidVolumePercent(value.master_volume_percent)) return null;
  if (typeof value.hotkeys_enabled !== "boolean") return null;
  if (value.hotkeys_registered !== null && typeof value.hotkeys_registered !== "boolean") return null;
  if (value.muted !== null && typeof value.muted !== "boolean") return null;
  return {
    master_volume_percent: value.master_volume_percent,
    hotkeys_enabled: value.hotkeys_enabled,
    hotkeys_registered: value.hotkeys_registered,
    muted: value.muted,
  };
}

export function parseAudioVolumeEvent(value: unknown): AudioVolumeEvent | null {
  if (!isRecord(value) || !hasExactKeys(value, VOLUME_EVENT_KEYS)) return null;
  if (!isValidVolumePercent(value.percent)) return null;
  if (value.source !== "ui" && value.source !== "hotkey") return null;
  return { percent: value.percent, source: value.source };
}

export interface AudioPrefsClient {
  get(): Promise<AudioPrefs | null>;
  setVolume(percent: number): Promise<AudioPrefs | null>;
  setHotkeys(enabled: boolean): Promise<AudioPrefs | null>;
  subscribeVolume(onEvent: (event: AudioVolumeEvent) => void): Promise<() => void>;
}

export interface AudioPrefsPorts {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
  listen(event: string, onPayload: (payload: unknown) => void): Promise<() => void>;
}

/** Typed renderer boundary. Host payloads are parsed before the shared state can see them. */
export function createAudioPrefsClient(ports: AudioPrefsPorts): AudioPrefsClient {
  const invokePrefs = async (command: string, args?: Record<string, unknown>): Promise<AudioPrefs | null> => {
    try {
      return parseAudioPrefs(await ports.invoke(command, args));
    } catch {
      return null;
    }
  };

  return {
    get: () => invokePrefs("audio_prefs_get"),
    setVolume: (percent) => isValidVolumePercent(percent)
      ? invokePrefs("audio_set_volume", { percent })
      : Promise.resolve(null),
    setHotkeys: (enabled) => typeof enabled === "boolean"
      ? invokePrefs("audio_set_hotkeys", { enabled })
      : Promise.resolve(null),
    subscribeVolume: (onEvent) => ports.listen("audio://volume", (payload) => {
      const event = parseAudioVolumeEvent(payload);
      if (event) onEvent(event);
    }),
  };
}

export interface TrailingThrottle<T> {
  push(value: T): void;
  dispose(): void;
}

/** Coalesce bursts and guarantee a trailing send no more often than the interval. */
export function createTrailingThrottle<T>(send: (value: T) => void, intervalMs = 120): TrailingThrottle<T> {
  let nextAllowedAt = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pending = false;
  let latest: T;

  const run = () => {
    timer = null;
    if (!pending) return;
    const wait = Math.max(0, nextAllowedAt - Date.now());
    if (wait > 0) {
      timer = setTimeout(run, wait);
      return;
    }
    pending = false;
    nextAllowedAt = Date.now() + intervalMs;
    send(latest);
  };

  return {
    push(value) {
      latest = value;
      pending = true;
      if (timer === null) timer = setTimeout(run, Math.max(0, nextAllowedAt - Date.now()));
    },
    dispose() {
      if (timer !== null) clearTimeout(timer);
      timer = null;
      pending = false;
    },
  };
}

export interface VolumeCopy {
  unknownLevel: string;
  mutedMessage: string;
  hotkeysRegistrationMessage: string;
}

export interface VolumeControlModel {
  unknown: boolean;
  percent: number | null;
  readout: string;
  mutedMessage: string | null;
  hotkeysEnabled: boolean | null;
  hotkeysRegistrationFailed: boolean;
  hotkeysRegistrationMessage: string | null;
}

export function volumeControlModel(
  prefs: AudioPrefs | null,
  draftPercent: number | null,
  text: VolumeCopy,
): VolumeControlModel {
  const unknown = prefs === null;
  const percent = unknown
    ? null
    : isValidVolumePercent(draftPercent) ? draftPercent : prefs.master_volume_percent;
  const hotkeysRegistrationFailed = prefs !== null
    && prefs.hotkeys_enabled === true
    && prefs.hotkeys_registered === false;
  return {
    unknown,
    percent,
    readout: percent === null ? text.unknownLevel : `${percent}%`,
    mutedMessage: prefs?.muted === true ? text.mutedMessage : null,
    hotkeysEnabled: prefs?.hotkeys_enabled ?? null,
    hotkeysRegistrationFailed,
    hotkeysRegistrationMessage: hotkeysRegistrationFailed ? text.hotkeysRegistrationMessage : null,
  };
}

/** Small reusable timeout owner for transient host-event notices. */
export class TransientNotice<T> {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private readonly onChange: (value: T | null) => void;
  private readonly durationMs: number;

  constructor(onChange: (value: T | null) => void, durationMs = 1_500) {
    this.onChange = onChange;
    this.durationMs = durationMs;
  }

  show(value: T): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.onChange(value);
    this.timer = setTimeout(() => {
      this.timer = null;
      this.onChange(null);
    }, this.durationMs);
  }

  dispose(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}

export type AudioPrefsError = "volume_failed" | "hotkeys_failed";

export interface AudioPrefsSnapshot {
  prefs: AudioPrefs | null;
  draftPercent: number | null;
  loading: boolean;
  hotkeysPending: boolean;
  error: AudioPrefsError | null;
  volumeEvent: { event: AudioVolumeEvent; sequence: number } | null;
}

interface VolumeRequest {
  percent: number;
  version: number;
}

const ERROR_VISIBLE_MS = 2_500;

/** One process-wide owner for preferences, edits, and volume events. */
export class AudioPrefsStore {
  private snapshot: AudioPrefsSnapshot = {
    prefs: null,
    draftPercent: null,
    loading: true,
    hotkeysPending: false,
    error: null,
    volumeEvent: null,
  };
  private readonly listeners = new Set<() => void>();
  private readonly volumeThrottle;
  private started = false;
  private operationQueue: Promise<void> = Promise.resolve();
  private volumeEditVersion = 0;
  private eventSequence = 0;
  private errorTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly client: AudioPrefsClient;

  constructor(client: AudioPrefsClient) {
    this.client = client;
    this.volumeThrottle = createTrailingThrottle<VolumeRequest>((request) => {
      void this.enqueue(() => this.persistVolume(request));
    }, 120);
  }

  readonly getSnapshot = (): AudioPrefsSnapshot => this.snapshot;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  start(): void {
    if (this.started) return;
    this.started = true;
    const eventSequenceAtRead = this.eventSequence;
    void this.client.get().then((reply) => {
      this.publish({ prefs: this.withLatestVolumeEvent(parseAudioPrefs(reply), eventSequenceAtRead), loading: false });
    }).catch(() => this.publish({ prefs: null, loading: false }));

    void this.client.subscribeVolume((reply) => {
      const event = parseAudioVolumeEvent(reply);
      if (!event) return;
      this.eventSequence += 1;
      const prefs = this.snapshot.prefs;
      this.publish({
        prefs: prefs ? { ...prefs, master_volume_percent: event.percent } : null,
        volumeEvent: { event, sequence: this.eventSequence },
      });
    }).catch(() => {
      // A missing event listener leaves the last confirmed preferences unchanged.
    });
  }

  setVolume(percent: number): void {
    if (!this.snapshot.prefs || !isValidVolumePercent(percent)) return;
    const version = ++this.volumeEditVersion;
    this.clearError();
    this.publish({ draftPercent: percent });
    this.volumeThrottle.push({ percent, version });
  }

  setHotkeys(enabled: boolean): void {
    if (!this.snapshot.prefs || this.snapshot.hotkeysPending || typeof enabled !== "boolean") return;
    this.clearError();
    this.publish({ hotkeysPending: true });
    void this.enqueue(async () => {
      const eventSequenceAtRequest = this.eventSequence;
      try {
        const prefs = parseAudioPrefs(await this.client.setHotkeys(enabled));
        if (!prefs) throw new Error("invalid audio hotkeys reply");
        this.publish({ prefs: this.withLatestVolumeEvent(prefs, eventSequenceAtRequest) });
      } catch {
        this.showError("hotkeys_failed");
      } finally {
        this.publish({ hotkeysPending: false });
      }
    });
  }

  private async persistVolume(request: VolumeRequest): Promise<void> {
    const eventSequenceAtRequest = this.eventSequence;
    try {
      const prefs = parseAudioPrefs(await this.client.setVolume(request.percent));
      if (!prefs) throw new Error("invalid audio volume reply");
      this.publish({
        prefs: this.withLatestVolumeEvent(prefs, eventSequenceAtRequest),
        draftPercent: request.version === this.volumeEditVersion ? null : this.snapshot.draftPercent,
      });
      if (request.version === this.volumeEditVersion) this.clearError();
    } catch {
      if (request.version === this.volumeEditVersion) {
        this.publish({ draftPercent: null });
        this.showError("volume_failed");
      }
    }
  }

  private enqueue(work: () => Promise<void>): Promise<void> {
    const result = this.operationQueue.then(work, work);
    this.operationQueue = result.catch(() => {});
    return result;
  }

  private clearError(): void {
    if (this.errorTimer !== null) clearTimeout(this.errorTimer);
    this.errorTimer = null;
    if (this.snapshot.error !== null) this.publish({ error: null });
  }

  private showError(error: AudioPrefsError): void {
    if (this.errorTimer !== null) clearTimeout(this.errorTimer);
    this.publish({ error });
    this.errorTimer = setTimeout(() => {
      this.errorTimer = null;
      this.publish({ error: null });
    }, ERROR_VISIBLE_MS);
  }

  private withLatestVolumeEvent(prefs: AudioPrefs | null, eventSequenceAtRequest: number): AudioPrefs | null {
    const latestEvent = this.snapshot.volumeEvent;
    return prefs && this.eventSequence > eventSequenceAtRequest && latestEvent
      ? { ...prefs, master_volume_percent: latestEvent.event.percent }
      : prefs;
  }

  private publish(patch: Partial<AudioPrefsSnapshot>): void {
    this.snapshot = { ...this.snapshot, ...patch };
    for (const listener of this.listeners) listener();
  }
}

type VolumeListener = (event: AudioVolumeEvent) => void;

/** Synthetic renderer-only implementation of the volume host contract. */
export class MockAudioPrefs {
  private prefs: AudioPrefs = {
    master_volume_percent: 60,
    hotkeys_enabled: true,
    hotkeys_registered: true,
    muted: false,
  };
  private registrationResult: boolean | null = true;
  private readonly listeners = new Set<VolumeListener>();

  async get(): Promise<AudioPrefs> {
    return { ...this.prefs };
  }

  async setVolume(percent: number): Promise<AudioPrefs> {
    if (!isValidVolumePercent(percent)) throw new Error("audio_set_volume requires an integer from 0 to 100");
    const changed = percent !== this.prefs.master_volume_percent;
    this.prefs = { ...this.prefs, master_volume_percent: percent };
    if (changed) this.emit({ percent, source: "ui" });
    return { ...this.prefs };
  }

  async setHotkeys(enabled: boolean): Promise<AudioPrefs> {
    if (typeof enabled !== "boolean") throw new Error("audio_set_hotkeys requires a boolean");
    this.prefs = {
      ...this.prefs,
      hotkeys_enabled: enabled,
      hotkeys_registered: enabled ? this.registrationResult : false,
    };
    return { ...this.prefs };
  }

  subscribeVolume(listener: VolumeListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  setRegistrationResult(registered: boolean | null): void {
    this.registrationResult = registered;
    if (this.prefs.hotkeys_enabled) this.prefs = { ...this.prefs, hotkeys_registered: registered };
  }

  changeFromHotkey(percent: number): boolean {
    if (!this.prefs.hotkeys_enabled || this.prefs.hotkeys_registered === false || !isValidVolumePercent(percent)) return false;
    if (percent !== this.prefs.master_volume_percent) {
      this.prefs = { ...this.prefs, master_volume_percent: percent };
      this.emit({ percent, source: "hotkey" });
    }
    return true;
  }

  private emit(value: AudioVolumeEvent): void {
    if (!parseAudioPrefs(this.prefs)) return;
    for (const listener of this.listeners) listener({ ...value });
  }
}
