// Shapes of runtime-status 1.1.0 and own-snapshot 2.0.0 contracts.

import type { Locale } from "./copy";

export interface UserSettingsView {
  revision: number;
  locale: Locale;
}

export type ErrorCode = string | null;

export type GsiEventId =
  | "multi_kill"
  | "streak"
  | "first_blood"
  | "hero_kill"
  | "hero_death"
  | "level_milestone"
  | "health_low"
  | "mana_low"
  | "match_ended"
  | "match_started"
  | "hero_respawn"
  | "health_danger";

export interface RuntimeStatus {
  schema_version: "1.1.0";
  status_revision: number;
  intake: {
    state: "not_configured" | "starting" | "listening" | "receiving" | "stale" | "unavailable";
    port: number | null;
    generation: number | null;
    last_admitted_age_ms: number | null;
    reason: ErrorCode;
  };
  ownership: { state: "unknown" | "verified" | "unverified" | "mismatch"; reason: ErrorCode };
  session: { phase: "none" | "draft" | "pregame" | "live" | "post_game" | "unsupported"; session_epoch: number | null };
  mode: { state: "unverified" | "identified" | "unsupported"; value: string | null };
  resource: {
    containment: "established";
    cpu: { state: "available" | "unavailable"; last_sample_percent: number | null; throttle: "normal" | "throttled" };
    memory: { state: "available" | "over_budget" | "unavailable"; working_set_bytes: number | null };
  };
  media: { state: "not_ready" | "ready" | "unavailable"; reason: ErrorCode };
  capture: { state: "available" | "unsupported" | "unavailable"; reason: ErrorCode };
  account: { state: "unavailable" | "signed_out" | "signed_in" };
  events: {
    detector: "idle" | "running";
    last_event_age_ms: number | null;
    unavailable: { id: GsiEventId; reason: "CAPABILITY_UNAVAILABLE" }[];
  };
}

export type FieldAvailability = "fresh" | "stale" | "unknown" | "invalid" | "unsupported";

export interface Field<T> {
  value: T | null;
  availability: FieldAvailability;
}

export interface OwnSnapshot {
  schema_version: "2.0.0";
  session_epoch: number | null;
  availability: FieldAvailability;
  source: { kind: "gsi"; generation: number; sequence: number | null; age_ms: number | null };
  own: null | {
    identity_revision: number;
    team: "radiant" | "dire" | null;
    fields: {
      hero_id: Field<string>;
      level: Field<number>;
      hp: Field<number>;
      max_hp: Field<number>;
      mana_percent: Field<number>;
      alive: Field<boolean>;
      respawn_seconds: Field<number>;
    };
  };
}
