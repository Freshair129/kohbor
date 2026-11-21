import { invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Locale } from "../copy";
import { DRAFT_HEIGHT, DRAFT_WIDTH, draftBarPresentation, parseDraftBarPacket, type DraftBarFrame } from "./draftBarModel";

const POLL_MS = 100;
const EXPIRE_MS = 1000;

export function useDraftBar(epoch: number | null): DraftBarFrame | null {
  const [sample, setSample] = useState<{ epoch: number; frame: DraftBarFrame } | null>(null);
  useEffect(() => {
    if (epoch === null || !isTauri()) return;
    const admittedEpoch = epoch;
    let mounted = true;
    let current: DraftBarFrame | null = null;
    let pollTimer: number | undefined;
    let expiryTimer: number | undefined;
    let visibilityRevision = 0;

    function replace(next: DraftBarFrame | null) {
      if (current?.bgra) new Uint8Array(current.bgra.buffer).fill(0);
      current = next;
      if (mounted) setSample(next ? { epoch: admittedEpoch, frame: next } : null);
    }
    function visibilityChanged() {
      visibilityRevision += 1;
      replace(null);
    }
    async function poll() {
      if (document.visibilityState === "visible") {
        const revision = visibilityRevision;
        try {
          const raw = await invoke<ArrayBuffer>("draft_bar_read", { expectedEpoch: admittedEpoch });
          if (mounted && revision === visibilityRevision && document.visibilityState === "visible") {
            const next = parseDraftBarPacket(raw);
            if (!next && raw instanceof ArrayBuffer) new Uint8Array(raw).fill(0);
            window.clearTimeout(expiryTimer);
            replace(next?.active ? next : null);
            expiryTimer = window.setTimeout(() => replace(null), EXPIRE_MS);
          } else if (raw instanceof ArrayBuffer) {
            new Uint8Array(raw).fill(0);
          }
        } catch {
          replace(null);
        }
      }
      if (mounted) pollTimer = window.setTimeout(() => { void poll(); }, POLL_MS);
    }

    document.addEventListener("visibilitychange", visibilityChanged);
    void poll();
    return () => {
      mounted = false;
      window.clearTimeout(pollTimer);
      window.clearTimeout(expiryTimer);
      document.removeEventListener("visibilitychange", visibilityChanged);
      replace(null);
      setSample(null);
    };
  }, [epoch]);
  return sample?.epoch === epoch ? sample.frame : null;
}

const DETAILS: Record<string, [string, string]> = {
  capture_disabled: ["Capture is disabled", "ปิดการจับภาพอยู่"],
  paused: ["Capture is paused", "พักการจับภาพอยู่"],
  capture_invalidated: ["Capture is awaiting a retry", "รอลองจับภาพใหม่"],
  no_game_window: ["Dota window not found", "ไม่พบหน้าต่าง Dota"],
  draft_window_not_aligned: ["Dota must fill the duplicated display", "หน้าต่าง Dota ต้องเต็มจอที่จับภาพ"],
  draft_window_visibility_unconfirmed: ["The draft strip is covered or its visibility could not be confirmed", "แถบดราฟต์ถูกบังหรือยืนยันการมองเห็นไม่ได้"],
  draft_grab_unavailable: ["Draft capture is unavailable", "จับภาพดราฟต์ไม่ได้"],
  capture_reopening: ["Reconnecting capture", "กำลังเชื่อมต่อการจับภาพ"],
  output_unavailable: ["Display unavailable", "ไม่พบจอสำหรับจับภาพ"],
};

export function DraftBarStrip({ frame, locale }: { frame: DraftBarFrame | null; locale: Locale }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const view = draftBarPresentation(frame, false, locale);
  useLayoutEffect(() => {
    const bgra = frame?.bgra;
    const context = canvas.current?.getContext("2d", { alpha: false });
    if (!context || !bgra) return;
    const image = context.createImageData(DRAFT_WIDTH, DRAFT_HEIGHT);
    for (let i = 0; i < bgra.length; i += 4) {
      image.data[i] = bgra[i + 2]!;
      image.data[i + 1] = bgra[i + 1]!;
      image.data[i + 2] = bgra[i]!;
      image.data[i + 3] = 255;
    }
    context.putImageData(image, 0, 0);
    image.data.fill(0);
    return () => context.clearRect(0, 0, DRAFT_WIDTH, DRAFT_HEIGHT);
  }, [frame]);

  if (!view) return null;
  const title = locale === "th" ? "แถบดราฟต์" : "Draft bar";
  const detail = view.detail === null ? null :
    (Object.hasOwn(DETAILS, view.detail) ? DETAILS[view.detail]![locale === "th" ? 1 : 0] : view.detail);
  return (
    <section className="deck-draft" aria-label={title}>
      {frame?.bgra
        ? <canvas ref={canvas} width={DRAFT_WIDTH} height={DRAFT_HEIGHT} aria-label={title} role="img" />
        : <p className="deck-draft-detail" role="status">{detail ?? (locale === "th" ? "รอภาพดราฟต์" : "Waiting for the draft frame")}</p>}
      <div className="deck-draft-labels">
        {view.slots.map((slot) => (
          <span key={slot.column} data-team={slot.team} style={{ gridColumn: slot.column }} title={slot.title}>
            {slot.text}
          </span>
        ))}
      </div>
    </section>
  );
}
