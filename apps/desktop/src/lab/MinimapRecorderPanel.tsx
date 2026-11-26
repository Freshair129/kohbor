import { useCallback, useEffect, useState } from "react";
import {
  formatRecordingBytes,
  recordingIndicatorState,
  type MinimapRecorderStatus,
} from "./minimapRecorderModel";
import {
  labMinimapRecorderDelete,
  labMinimapRecorderOpenFolder,
  labMinimapRecorderSet,
  labMinimapRecorderStatus,
} from "./labRuntime";

const POLL_MS = 1000;
const GB = 1_000_000_000;

export function MinimapRecorderPanel() {
  const [status, setStatus] = useState<MinimapRecorderStatus | null>(null);
  const [capGb, setCapGb] = useState("5");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const next = await labMinimapRecorderStatus();
    if (next !== null) setStatus(next);
  }, []);

  useEffect(() => {
    let active = true;
    const poll = async () => {
      const next = await labMinimapRecorderStatus();
      if (active && next !== null) setStatus(next);
    };
    void poll();
    const timer = window.setInterval(poll, POLL_MS);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (status !== null) setCapGb((status.total_cap_bytes / GB).toString());
  }, [status?.total_cap_bytes]);

  const update = async (enabled: boolean, samplingRate = status?.sampling_rate_fps ?? 2, capBytes = status?.total_cap_bytes ?? 5 * GB) => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await labMinimapRecorderSet({
        enabled,
        sampling_rate_fps: samplingRate,
        total_cap_bytes: capBytes,
      }));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const indicatorState = recordingIndicatorState(status);
  const recording = indicatorState === "recording";
  if (status === null) return null;

  return (
    <section className="lab-minimap-recorder" aria-labelledby="lab-minimap-recorder-title">
      <header className="lab-minimap-recorder-head">
        <div>
          <h3 id="lab-minimap-recorder-title">Minimap test recordings</h3>
          <p>บันทึกเฉพาะภาพมินิแมปในเครื่อง เพื่อใช้ทำชุดข้อมูลทดสอบ Lab</p>
        </div>
        {indicatorState !== null && <span className="lab-minimap-recorder-indicator" role="status">{recording ? "กำลังบันทึกมินิแมป (Lab)" : "เปิดการบันทึกมินิแมป (Lab)"}</span>}
      </header>

      <label className="lab-minimap-recorder-toggle">
        <input
          type="checkbox"
          role="switch"
          checked={status.enabled}
          disabled={busy}
          onChange={(event) => void update(event.currentTarget.checked)}
        />
        <span>บันทึกภาพมินิแมปเพื่อทดสอบ (Lab)</span>
      </label>

      <div className="lab-minimap-recorder-controls">
        <label>
          Sampling rate
          <select
            value={status.sampling_rate_fps}
            disabled={busy}
            onChange={(event) => void update(status.enabled, Number(event.currentTarget.value))}
          >
            {[1, 2, 3, 4, 5].map((fps) => <option value={fps} key={fps}>{fps} fps</option>)}
          </select>
        </label>
        <label>
          Total storage cap (GB)
          <input
            type="number"
            min="0.1"
            step="0.1"
            value={capGb}
            disabled={busy}
            onChange={(event) => setCapGb(event.currentTarget.value)}
            onBlur={() => {
              const value = Number(capGb);
              if (Number.isFinite(value) && value > 0) void update(status.enabled, status.sampling_rate_fps, Math.max(1, Math.floor(value * GB)));
            }}
          />
        </label>
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            setError(null);
            void labMinimapRecorderOpenFolder().catch((reason: unknown) => setError(String(reason)));
          }}
        >
          Open recordings folder
        </button>
      </div>

      {!status.enabled && <p>ปิดอยู่ตามค่าเริ่มต้น จะไม่บันทึกเฟรมจนกว่าจะเปิดสวิตช์นี้</p>}
      {status.enabled && !recording && <p>รอแมตช์ Live, capture สด และยืนยัน crop ก่อนเริ่มบันทึก</p>}
      {status.stop_reason === "cap_reached" && <p role="alert">พื้นที่ถึงขีดจำกัด หยุดบันทึกแมตช์นี้แล้ว</p>}
      {status.dropped_frames > 0 && <p>เฟรมที่ queue เต็มแล้วข้าม: {status.dropped_frames}</p>}
      <p>ชุดบันทึก {status.recordings_count} · ใช้ {formatRecordingBytes(status.total_bytes)} จาก {formatRecordingBytes(status.total_cap_bytes)}</p>
      <p>ขีดจำกัดภาพต่อแมตช์ {formatRecordingBytes(status.per_match_frame_cap_bytes)}</p>

      {error && <p role="alert">{error}</p>}
      <ul className="lab-minimap-recorder-list">
        {status.matches.map((match, index) => (
          <li key={match.match_hash}>
            <span>Match {index + 1} · {match.frame_count} frames · {formatRecordingBytes(match.total_bytes)}</span>
            <button
              type="button"
              disabled={busy}
              onClick={() => {
                if (!window.confirm("ลบชุดบันทึกมินิแมปนี้หรือไม่? การลบนี้ย้อนกลับไม่ได้")) return;
                setBusy(true);
                setError(null);
                void labMinimapRecorderDelete(match.match_hash)
                  .then(setStatus)
                  .catch((reason: unknown) => setError(String(reason)))
                  .finally(() => setBusy(false));
              }}
            >
              ลบชุดนี้
            </button>
          </li>
        ))}
      </ul>
      <button className="lab-minimap-recorder-refresh" type="button" onClick={() => void refresh()}>
        Refresh recordings
      </button>
    </section>
  );
}
