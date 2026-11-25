import { useState } from "react";
import type { EventSound, EventMapLoadout } from "./eventMap";
import type { PackSummary } from "./model";
import { soundLabel, soundKey } from "./eventMap";
import { mediaAuditionInstalledVariant } from "../runtime";
import "./CoverageMatrix.css";

export function CoverageMatrix({ rows, loadout, pending, packs, prewarm, disabled, locale, onLock, onResult }:{
  rows: readonly EventSound[]; loadout: EventMapLoadout; packs: readonly PackSummary[];
  pending?: EventMapLoadout | null;
  prewarm: { used_samples:number; cap_samples:number; skipped_variants:number } | undefined;
  disabled: boolean; locale:"th"|"en"; onLock:(row:EventSound, packageId:string|null)=>void; onResult:(row:EventSound)=>void;
}) {
  const [gapsOnly,setGapsOnly] = useState(false);
  const th = locale === "th";
  const name = (installId:string) => packs.find(pack => pack.installId === installId)?.name ?? `Pack ${loadout.equipped.findIndex(pack => pack.install_id === installId)+1}`;
  const activeRows = rows.filter(row => row.event_group === "basic");
  const specialRows = rows.filter(row => row.event_group === "special");
  const covered = rows.filter(row => row.sources.some(source => source.state === "ready")).length;
  const memory = prewarm ? `${(prewarm.used_samples * 2 / 1_000_000).toFixed(1)} / ${(prewarm.cap_samples * 2 / 1_000_000).toFixed(0)} MB` : "—";

  const renderRows = (group: string, groupRows: readonly EventSound[]) => groupRows.filter(row => !gapsOnly || !row.sources.some(source => source.state === "ready")).map(row => {
    const key = soundKey(row);
    const label = soundLabel(row, locale);
    const readyVariants = row.variants.filter(variant => variant.state === "ready");
    const missingReason = row.state === "detector_unavailable" ? (th ? "ยังตรวจจับเหตุการณ์นี้ไม่ได้" : "Detector not available")
      : row.state === "not_voiced" ? (th ? "ไม่มีเสียงพากย์" : "Not voiced") : row.state === "not_loaded" ? (th ? "ไม่โหลดเพราะงบหน่วยความจำ" : "Not loaded: memory budget")
      : row.state === "invalid_media" ? (th ? "ไฟล์เสียงมีปัญหา" : "File problem")
      : row.state === "unmapped" ? (th ? "ไม่มีแพ็กที่ติดตั้งซึ่งมีเสียงนี้" : "No equipped pack has this sound") : row.reason;
    return <tr key={key} data-group={group}>
      <th scope="row">{label.title}{label.tier && <small> · {label.tier}</small>}</th>
      {loadout.equipped.map(pack => {
        const source = row.sources.find(item => item.install_id === pack.install_id);
        const variants = row.variants.filter(item => item.install_id === pack.install_id);
        const ready = variants.filter(item => item.state === "ready");
        const notLoaded = variants.some(item => item.state === "not_loaded");
        const failed = variants.some(item => item.state === "invalid_media");
        return <td key={pack.install_id} aria-label={source ? `${ready.length} ready voices` : "No sound"}>
          {source ? <>
            {ready.length > 0 && <span>{ready.length} {th ? "เสียง" : `voice${ready.length === 1 ? "" : "s"}`}</span>}
            {notLoaded && <small role="status">{th ? "ไม่โหลดเพราะงบหน่วยความจำ" : "Not loaded: memory budget"}</small>}
            {failed && <small role="status">{th ? "ไฟล์เสียงมีปัญหา" : "File problem"}</small>}
            {ready.map(variant => <button key={variant.variant_id} type="button" disabled={disabled}
              aria-label={`${th ? "ฟังตัวอย่าง" : "Preview"} ${variant.caption ?? label.title} ${th ? "จาก" : "from"} ${name(pack.install_id)}`}
              onClick={() => void mediaAuditionInstalledVariant(pack.install_id, row.event_id, row.tier, variant.variant_id)}>
              ▶ <small>{variant.caption ?? (th ? `เสียง ${variant.variant_id}` : `Voice ${variant.variant_id}`)}</small></button>)}
          </> : <span aria-label="No sound here">—</span>}
        </td>;
      })}
      <td>
        {row.state === "detector_unavailable" && <span role="status">{missingReason}</span>}
        {row.lock_state === "unsatisfied" && <span role="status">{th ? "ล็อกไม่ได้: ใช้เสียงผสมแทน" : "Lock not possible: mixing instead"}</span>}
        {row.lock ? <span>{th ? `ล็อก: ${row.lock}` : `Locked: ${row.lock}`}</span> : row.sources.length > 1 ? <span>{th ? `ผสม ${row.sources.length} แพ็ก (${readyVariants.length} เสียง)` : `Mix of ${row.sources.length} packs (${readyVariants.length} voices)`}</span>
          : row.sources.length === 1 ? <span>{name(row.sources[0]!.install_id)}</span> : <span role="status">{th ? `เงียบ: ${missingReason}` : `Silent: ${missingReason}`}</span>}
        {readyVariants.length > 0 && <button type="button" disabled={disabled} onClick={() => onResult(row)}>{th ? "ฟังผลลัพธ์" : "Play result"}</button>}
      </td>
      <td>{row.sources.length > 1 || row.lock_state === "unsatisfied" ? <select aria-label={th ? `ล็อก${label.title}กับแพ็ก` : `Lock ${label.title} to a pack`}
        value={row.lock ?? ""} disabled={disabled} onChange={event => onLock(row, event.currentTarget.value || null)}>
        <option value="">{th ? "อัตโนมัติ (ผสม)" : "Auto (mix)"}</option>
        {row.sources.map(source => <option key={source.package_id} value={source.package_id}>{name(source.install_id)}</option>)}
        {row.lock_state === "unsatisfied" && row.lock && !row.sources.some(source => source.package_id === row.lock)
          && <option value={row.lock}>{row.lock} · {th ? "ไม่มีเสียง" : "no sound here"}</option>}
      </select> : <span>—</span>}</td>
    </tr>;
  });

  return <section className="packs-coverage" aria-label="Event sound coverage">
    <header><strong>{th ? `มีเสียง ${covered} จาก ${rows.length} เหตุการณ์` : `Covered ${covered} of ${rows.length}`}</strong><span>{th ? `เงียบ ${rows.length-covered}` : `${rows.length-covered} silent`}</span><label><input type="checkbox" checked={gapsOnly} onChange={event => setGapsOnly(event.currentTarget.checked)} />{th ? "แสดงเฉพาะช่องว่าง" : "Show gaps only"}</label><span>{th ? "หน่วยความจำ" : "Memory"} {memory}</span></header>
    {pending && <p role="status">{th ? "ชุดแพ็กที่รอ: " : "Pending loadout: "}{pending.equipped.map((pack, slot) => `${slot + 1}. ${name(pack.install_id)}`).join(" · ") || (th ? "ไม่มีแพ็ก" : "No packs")}</p>}
    <p className="packs-coverage-hint">{th ? "ช่องที่ 1 มีลำดับก่อน เมื่อลำดับนั้นไม่มีเสียง ระบบจะข้ามไปยังแพ็กถัดไปที่มีเสียง" : "Slot 1 is checked first. If it has no sound for an event, playback skips to the next equipped pack with coverage."}</p>
    {prewarm && prewarm.skipped_variants > 0 && <p role="status">{th ? `${prewarm.skipped_variants} คลิปไม่ได้โหลดเพราะงบหน่วยความจำ ดูข้อความ “ไม่โหลด” ในคอลัมน์แพ็ก` : `${prewarm.skipped_variants} clips were skipped by the memory budget. See “Not loaded” in the pack columns.`}</p>}
    <div className="packs-coverage-scroll"><table><thead><tr><th scope="col">{th ? "เหตุการณ์" : "Event"}</th>
      {loadout.equipped.map((pack,slot)=><th scope="col" key={pack.install_id}>{slot+1} · {name(pack.install_id)}</th>)}
      <th scope="col">{th ? "ผลการเล่น" : "Plays"}</th><th scope="col">{th ? "ล็อกแพ็ก" : "Lock"}</th></tr></thead><tbody>
      {(!gapsOnly || activeRows.some(row => !row.sources.some(source => source.state === "ready"))) && <tr className="packs-coverage-group"><th colSpan={loadout.equipped.length+3}>{th ? "พื้นฐาน" : "BASIC"}</th></tr>}{renderRows("basic",activeRows)}
      {(!gapsOnly || specialRows.some(row => !row.sources.some(source => source.state === "ready"))) && <tr className="packs-coverage-group"><th colSpan={loadout.equipped.length+3}>{th ? "พิเศษ" : "SPECIAL"}</th></tr>}{renderRows("special",specialRows)}
    </tbody></table></div>
  </section>;
}
