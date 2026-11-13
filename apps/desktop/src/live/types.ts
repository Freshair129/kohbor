export type CapturePhase =
  | "unsupported"
  | "calibrating"
  | "live"
  | "stale"
  | "coverage_unavailable"
  | "lost"
  | "unknown";

export interface CaptureStatus {
  phase: CapturePhase;
  fps_measured: number | null;
  grab_fps: number | null;
  sent_total: number | null;
  grab_total: number | null;
  frame_age_ms: number | null;
  rect_profile: string | null;
  calibration_generation: number | null;
  auto_confirmed: boolean;
  crop_confirmed: boolean;
  display_fps: 10 | 15 | 30 | null;
  paused: boolean | null;
  phase_reason: string | null;
  grab_ms_mean: number | null;
  analysis_ms_mean: number | null;
}

export type VisionEvent = "enemy_missing" | "risk_alert" | "risk_revision";
export type RiskEvent = VisionEvent | "risk_unavailable";

export interface RiskState {
  meter: number | null;
  gsi_source: boolean;
  vision_source: boolean;
  visible: number | null;
  baseline: number | null;
  events: RiskEvent[];
}

export interface RiskView {
  state: RiskState | null;
  lastVisionEvent: VisionEvent | null;
}

export interface ResourceMetrics {
  appGroupBytes: number | null;
  hostGpuBytes: number | null;
  webviewGpuBytes: number | null;
  gpuLoadPercent: number | null;
  cpuPercent: number | null;
  captureFps: number | null;
  frameAgeMs: number | null;
}

export type CaptureCostMetricKey = "grabMsMean" | "analysisMsMean";
export type ResourceMetricKey = keyof ResourceMetrics | CaptureCostMetricKey;

export interface MetricSummary {
  peak: number | null;
  mean: number | null;
  count: number;
}

export type ResourceSummary = Record<ResourceMetricKey, MetricSummary>;

export interface FrameHeader {
  flags: number;
  width: number;
  height: number;
  stride: number;
  session_epoch: number;
  calibration_generation: number;
  frame_seq: number;
  age_ms: number;
  byte_length: number;
}

export interface ParsedFrame {
  header: FrameHeader;
  bgra: Uint8Array;
}
