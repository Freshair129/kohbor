export type FooterTone = "fresh" | "warn" | "stale" | "off" | "unknown";
export type FooterMetric = {
  id: "cpu" | "ram" | "capture" | "received" | "painted";
  label: string;
  value: number | null;
  display: string;
  unit: string;
  barPercent: number;
  tone: FooterTone;
  title: string;
};

export function footerMetric(input: Omit<FooterMetric, "barPercent"> & { barPercent?: number }): FooterMetric {
  const available = input.value !== null && Number.isFinite(input.value);
  return {
    ...input,
    value: available ? input.value : null,
    display: available ? input.display : "—",
    barPercent: available ? Math.min(100, Math.max(0, input.barPercent ?? 0)) : 0,
  };
}

export function footerStateLabel(capturePhase: string | null, locale: "th" | "en"): string {
  if (capturePhase === "live") return locale === "th" ? "สด" : "Live";
  if (capturePhase === "idle") return locale === "th" ? "ว่าง" : "Idle";
  if (capturePhase === "calibrating") return locale === "th" ? "ปรับเทียบ" : "Setup";
  if (capturePhase === "stale") return locale === "th" ? "เก่า" : "Stale";
  if (capturePhase === "lost") return locale === "th" ? "ขาด" : "Lost";
  if (capturePhase === "coverage_unavailable") return locale === "th" ? "ไม่มีภาพ" : "No image";
  if (capturePhase === "unsupported") return locale === "th" ? "ไม่รองรับ" : "Unsupported";
  if (capturePhase === "unknown") return locale === "th" ? "ไม่ทราบ" : "Unknown";
  return "—";
}
