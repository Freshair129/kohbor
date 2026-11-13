export interface CaptureRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface CropOffsets {
  dx: number;
  dy: number;
  dw: number;
  dh: number;
}

export type CaptureDisplayRate = 10 | 15 | 30;
export type CaptureProposalStatus = "none" | "searching" | "found" | "not_found";

export interface CaptureConfig {
  schema_version: 1;
  crop: CropOffsets;
  display_fps: CaptureDisplayRate;
  paused: boolean;
  profile_rect: CaptureRect | null;
  effective_rect: CaptureRect | null;
  proposal: CaptureRect | null;
  proposal_status: CaptureProposalStatus;
  crop_confirmed: boolean;
}

export interface CaptureConfigPatch {
  crop?: Partial<CropOffsets>;
  display_fps?: CaptureDisplayRate;
  paused?: boolean;
}

export interface CropNudge {
  dx: number;
  dy: number;
}

export const CAPTURE_DISPLAY_RATES = [10, 15, 30] as const satisfies readonly CaptureDisplayRate[];

const MIN_DIMENSION = 64;
const MAX_DIMENSION = 400;
const MAX_OFFSET = 200;
const PROPOSAL_STATUSES = new Set<CaptureProposalStatus>(["none", "searching", "found", "not_found"]);

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function integer(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value);
}

function isRect(value: unknown, bounded: boolean): value is CaptureRect {
  if (!record(value)) return false;
  const { x, y, width, height } = value;
  if (!integer(x) || !integer(y) || !integer(width) || !integer(height)) return false;
  if (width <= 0 || height <= 0) return false;
  return !bounded || (
    width >= MIN_DIMENSION && width <= MAX_DIMENSION &&
    height >= MIN_DIMENSION && height <= MAX_DIMENSION
  );
}

function isNullableRect(value: unknown, bounded: boolean): value is CaptureRect | null {
  return value === null || isRect(value, bounded);
}

function isCropOffsets(value: unknown): value is CropOffsets {
  if (!record(value)) return false;
  const { dx, dy, dw, dh } = value;
  return integer(dx) && integer(dy) && integer(dw) && integer(dh) &&
    Math.abs(dx) <= MAX_OFFSET && Math.abs(dy) <= MAX_OFFSET &&
    Math.abs(dw) <= MAX_OFFSET && Math.abs(dh) <= MAX_OFFSET;
}

function isDisplayRate(value: unknown): value is CaptureDisplayRate {
  return value === 10 || value === 15 || value === 30;
}

/** Returns a validated copy of the host JSON, ignoring unknown future fields. */
export function parseCaptureConfig(input: unknown): CaptureConfig | null {
  if (!record(input) || input.schema_version !== 1 || !isCropOffsets(input.crop)) return null;
  if (!isDisplayRate(input.display_fps) || typeof input.paused !== "boolean") return null;
  if (!isNullableRect(input.profile_rect, false) || !isNullableRect(input.effective_rect, true)) return null;
  if (!isNullableRect(input.proposal, true)) return null;
  if (typeof input.proposal_status !== "string" || !PROPOSAL_STATUSES.has(input.proposal_status as CaptureProposalStatus)) return null;
  if (typeof input.crop_confirmed !== "boolean") return null;

  const profile = input.profile_rect;
  const effective = input.effective_rect;
  if (profile && effective) {
    if (
      effective.x !== profile.x + input.crop.dx || effective.y !== profile.y + input.crop.dy ||
      effective.width !== profile.width + input.crop.dw || effective.height !== profile.height + input.crop.dh
    ) return null;
  }

  return {
    schema_version: 1,
    crop: { dx: input.crop.dx, dy: input.crop.dy, dw: input.crop.dw, dh: input.crop.dh },
    display_fps: input.display_fps,
    paused: input.paused,
    profile_rect: profile ? { ...profile } : null,
    effective_rect: effective ? { ...effective } : null,
    proposal: input.proposal ? { ...input.proposal } : null,
    proposal_status: input.proposal_status as CaptureProposalStatus,
    crop_confirmed: input.crop_confirmed,
  };
}

/** True only when the entire config satisfies the capture host contract. */
export function validateCaptureConfig(input: unknown): input is CaptureConfig {
  return parseCaptureConfig(input) !== null;
}

function clampOffset(value: number): number {
  return Math.max(-MAX_OFFSET, Math.min(MAX_OFFSET, value));
}

/** Build a partial crop patch for a signed move from the current relative offsets. */
export function nudge(dx: number, dy: number, step: CropNudge): CaptureConfigPatch {
  return { crop: { dx: clampOffset(dx + step.dx), dy: clampOffset(dy + step.dy) } };
}

function resize(config: CaptureConfig, step: number, direction: 1 | -1): CaptureConfigPatch | null {
  const profile = config.profile_rect;
  if (!profile || !Number.isInteger(step) || step <= 0) return null;

  const minWidth = Math.max(MIN_DIMENSION, profile.width - MAX_OFFSET);
  const maxWidth = Math.min(MAX_DIMENSION, profile.width + MAX_OFFSET);
  const minHeight = Math.max(MIN_DIMENSION, profile.height - MAX_OFFSET);
  const maxHeight = Math.min(MAX_DIMENSION, profile.height + MAX_OFFSET);
  if (minWidth > maxWidth || minHeight > maxHeight) return null;

  const width = Math.max(minWidth, Math.min(maxWidth, profile.width + config.crop.dw + direction * step));
  const height = Math.max(minHeight, Math.min(maxHeight, profile.height + config.crop.dh + direction * step));
  return { crop: { dw: width - profile.width, dh: height - profile.height } };
}

export function grow(config: CaptureConfig, step: number): CaptureConfigPatch | null {
  return resize(config, step, 1);
}

export function shrink(config: CaptureConfig, step: number): CaptureConfigPatch | null {
  return resize(config, step, -1);
}

/** Convert an absolute proposal to offsets relative to the selected capture profile. */
export function applyProposal(proposal: CaptureRect, profile: CaptureRect | null): CaptureConfigPatch | null {
  if (!profile || !isRect(proposal, true) || !isRect(profile, false)) return null;
  const crop = {
    dx: proposal.x - profile.x,
    dy: proposal.y - profile.y,
    dw: proposal.width - profile.width,
    dh: proposal.height - profile.height,
  };
  if (!isCropOffsets(crop)) return null;
  return { crop };
}

export function togglePause(paused: boolean): CaptureConfigPatch {
  return { paused: !paused };
}

export function isCapturePaused(configPaused: boolean | null | undefined, statusPaused: boolean | null): boolean {
  return configPaused === true || statusPaused === true;
}

export function capturePictureEnabled(phase: string | null, paused: boolean): boolean {
  return !paused && (phase === "live" || phase === "stale");
}
