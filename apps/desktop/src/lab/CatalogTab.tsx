import { useCallback, useEffect, useMemo, useState } from "react";
import { demoAvailable, demoTrigger } from "../runtime";
import {
  ALL,
  CATEGORY_TH,
  EMPTY_FILTER,
  POSSIBLE_STATUS_TH,
  distinct,
  filterRows,
  gMaidenLabel,
  lastFiredLabel,
  summarize,
  tunablesLabel,
  type CatalogFilter,
  type CatalogView,
} from "./labCatalog";
import {
  DRAFT_PROBLEM_TH,
  bindingLabel,
  bindingsFor,
  clipChoices,
  draftProblem,
  playAvailability,
  type ActiveBindings,
  type DraftBinding,
  type MappableEvent,
} from "./labMapping";
import {
  labCatalog,
  labPackBindings,
  labPackMappingApply,
  labPackMappingExport,
  labPackMappingGet,
} from "./labRuntime";

const REFRESH_MS = 2000;

const SOURCE_TH: Record<string, string> = {
  gsi: "GSI",
  gsi_cv: "GSI + กล้อง CV",
  native_risk: "native risk (GSI + CV)",
  manual: "กดเอง",
  test_only: "ทดสอบเท่านั้น",
};

const STATUS_TH: Record<string, string> = {
  detected_today: "ตรวจจับได้แล้ว",
  catalog_only: "อยู่ในแคตตาล็อก ยังไม่สร้าง",
  test_only: "เฉพาะทดสอบ",
};

/** Dev-only table of every event: what we have, how it is detected, and what it maps to. */
export function CatalogTab({
  onOpenEvent,
  knownEditable,
}: {
  onOpenEvent: (id: string) => void;
  knownEditable: string[];
}) {
  const [view, setView] = useState<CatalogView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<CatalogFilter>(EMPTY_FILTER);
  const [pack, setPack] = useState<ActiveBindings | null>(null);
  const [packError, setPackError] = useState<string | null>(null);
  const [demoOn, setDemoOn] = useState(false);
  const [draft, setDraft] = useState<DraftBinding[]>([]);
  const [entry, setEntry] = useState<{ eventId: string; tier: string; audio: string }>({ eventId: "", tier: "", audio: "" });
  const [note, setNote] = useState<string | null>(null);
  const [exported, setExported] = useState<string | null>(null);

  const loadPack = useCallback(() => {
    void labPackBindings()
      .then((next) => {
        setPack(next);
        setPackError(null);
      })
      .catch((reason: unknown) => setPackError(String(reason)));
  }, []);

  useEffect(() => {
    loadPack();
    void demoAvailable().then(setDemoOn);
    void labPackMappingGet()
      .then(setDraft)
      .catch((reason: unknown) => setNote(String(reason)));
  }, [loadPack]);

  useEffect(() => {
    let active = true;
    const load = () => {
      void labCatalog()
        .then((next) => {
          if (!active) return;
          setView(next);
          setError(null);
        })
        .catch((reason: unknown) => active && setError(String(reason)));
    };
    load();
    const timer = window.setInterval(load, REFRESH_MS);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  const rows = useMemo(() => (view ? filterRows(view.rows, filter) : []), [view, filter]);

  if (view === null) return <p role="status">{error ?? "กำลังโหลดแคตตาล็อก…"}</p>;
  const totals = summarize(view.rows);
  const mappable: MappableEvent[] = view.rows
    .filter((row) => row.mediaMappable)
    .map((row) => ({ id: row.id, tierRange: row.tierRange }));
  const clips = pack ? clipChoices(pack.bindings) : [];
  const chosen = mappable.find((event) => event.id === entry.eventId);
  const candidate: DraftBinding = {
    eventId: entry.eventId,
    tier: chosen?.tierRange && entry.tier !== "" ? Number(entry.tier) : null,
    audio: entry.audio,
  };
  const problem = entry.eventId === "" || entry.audio === "" ? null : draftProblem(candidate, mappable, draft);

  function bindingCell(eventId: string) {
    if (packError !== null) return <span className="lab-muted">อ่าน pack ไม่ได้: {packError}</span>;
    if (pack === null) return <span className="lab-muted">กำลังอ่าน…</span>;
    if (pack.active === null) return <span className="lab-muted">ไม่มี pack ที่ใช้งาน</span>;
    const found = bindingsFor(eventId, pack.bindings);
    if (found.length === 0) return <span className="lab-muted">ไม่ได้ผูก (unmapped)</span>;
    return (
      <ul className="lab-bindings">
        {found.map((binding) => {
          const play = playAvailability(binding, demoOn);
          return (
            <li key={`${binding.eventId}-${binding.tier ?? "-"}`}>
              {bindingLabel(binding)}{" "}
              <button
                type="button"
                disabled={!play.ok}
                title={play.ok ? "เล่นผ่านเส้นทางเสียงจริง (demo trigger) แบนเนอร์จะขึ้นด้วย" : play.reason}
                onClick={() => {
                  void demoTrigger(binding.eventId, binding.tier).catch((reason: unknown) => setNote(String(reason)));
                }}
              >
                เล่น
              </button>
            </li>
          );
        })}
      </ul>
    );
  }

  const select = (key: "category" | "source" | "status", labels: Record<string, string>) => (
    <label>
      {key === "category" ? "หมวด" : key === "source" ? "แหล่งตรวจจับ" : "สถานะ"}
      <select value={filter[key]} onChange={(event) => setFilter({ ...filter, [key]: event.currentTarget.value })}>
        <option value={ALL}>ทั้งหมด</option>
        {distinct(view.rows, key).map((value) => (
          <option key={value} value={value}>
            {labels[value] ?? value}
          </option>
        ))}
      </select>
    </label>
  );

  return (
    <section className="lab-catalog" aria-labelledby="lab-catalog-title">
      <h3 id="lab-catalog-title">แคตตาล็อก event ทั้งหมด</h3>
      <p>
        ทั้งหมด {totals.total} event · ตรวจจับได้แล้ว {totals.detected} · อยู่ในแคตตาล็อกแต่ยังไม่สร้าง{" "}
        {totals.catalogOnly} · เฉพาะทดสอบ {totals.testOnly}
      </p>
      <p className="lab-note">{view.noteTh}</p>
      <div className="lab-filters">
        {select("category", CATEGORY_TH)}
        {select("source", SOURCE_TH)}
        {select("status", STATUS_TH)}
        <label>
          ค้นหา
          <input
            type="search"
            value={filter.text}
            onChange={(event) => setFilter({ ...filter, text: event.currentTarget.value })}
          />
        </label>
      </div>

      <div className="lab-table-wrap" tabIndex={0} aria-label="ตาราง event">
        <table className="lab-table">
          <thead>
            <tr>
              <th>id</th>
              <th>ชื่อไทย</th>
              <th>หมวด</th>
              <th>แหล่งตรวจจับ</th>
              <th>สถานะ</th>
              <th>G-Maiden</th>
              <th>ค่าที่ปรับได้</th>
              <th>คลิป/แบนเนอร์ใน pack ที่ใช้อยู่</th>
              <th>ยิงล่าสุด (เซสชันนี้)</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.id} className={`lab-status-${row.status}`}>
                <td>
                  <code>{row.id}</code>
                </td>
                <td>{row.labelTh}</td>
                <td>{CATEGORY_TH[row.category] ?? row.category}</td>
                <td>{row.sourceLabelTh}</td>
                <td>{row.statusTh}</td>
                <td>{gMaidenLabel(row.gMaiden)}</td>
                <td>
                  {tunablesLabel(row)}{" "}
                  {knownEditable.includes(row.id) && (
                    <button type="button" onClick={() => onOpenEvent(row.id)}>
                      ดู/แก้
                    </button>
                  )}
                </td>
                <td>{bindingCell(row.id)}</td>
                <td>{lastFiredLabel(row.lastFired)}</td>
              </tr>
            ))}
            {rows.length === 0 && (
              <tr>
                <td colSpan={9}>ไม่มี event ตรงกับตัวกรอง</td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <h4>จับคู่ event กับคลิป (ร่างสำหรับ dev)</h4>
      <p className="lab-note">
        {pack?.active
          ? `pack ที่ใช้อยู่: ${pack.active.packageId} (revision ${pack.active.revision})`
          : "ยังไม่มี pack ที่ใช้งาน"}
        . ปุ่ม "เล่น" เล่นได้เฉพาะคลิปที่ผูกอยู่จริง (ผ่าน demo trigger) เล่นคลิปของร่างหรือ pack อื่นไม่ได้ ร่างนี้ใช้ export
        ไปสร้าง pack ใหม่เท่านั้น ไม่เปลี่ยนเสียงในเกมทันที ชื่อไฟล์ใน pack ตั้งตาม event (ชื่อเดิมของคลิปไม่ถูกเก็บไว้ในแอป)
      </p>
      <div className="lab-filters">
        <label>
          event
          <select value={entry.eventId} onChange={(event) => setEntry({ eventId: event.currentTarget.value, tier: "", audio: entry.audio })}>
            <option value="">เลือก</option>
            {mappable.map((event) => (
              <option key={event.id} value={event.id}>
                {event.id}
              </option>
            ))}
          </select>
        </label>
        {chosen?.tierRange && (
          <label>
            tier ({chosen.tierRange[0]}..{chosen.tierRange[1]})
            <input
              type="number"
              min={chosen.tierRange[0]}
              max={chosen.tierRange[1]}
              value={entry.tier}
              onChange={(event) => setEntry({ ...entry, tier: event.currentTarget.value })}
            />
          </label>
        )}
        <label>
          คลิปของ pack ที่ใช้อยู่
          <select value={entry.audio} onChange={(event) => setEntry({ ...entry, audio: event.currentTarget.value })}>
            <option value="">เลือก</option>
            {clips.map((clip) => (
              <option key={clip} value={clip}>
                {clip}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          disabled={entry.eventId === "" || entry.audio === "" || problem !== null}
          title={problem ? DRAFT_PROBLEM_TH[problem] : undefined}
          onClick={() => {
            setDraft([...draft, candidate]);
            setEntry({ eventId: "", tier: "", audio: "" });
          }}
        >
          เพิ่มลงร่าง
        </button>
        {problem && <span className="lab-warn">{DRAFT_PROBLEM_TH[problem]}</span>}
      </div>
      <ul className="lab-bindings">
        {draft.map((item, index) => (
          <li key={`${item.eventId}-${item.tier ?? "-"}`}>
            {item.eventId}
            {item.tier === null ? "" : ` tier ${item.tier}`} → {item.audio}{" "}
            <button type="button" onClick={() => setDraft(draft.filter((_, i) => i !== index))}>
              เอาออก
            </button>
          </li>
        ))}
        {draft.length === 0 && <li className="lab-muted">ร่างว่าง</li>}
      </ul>
      <div className="lab-buttons">
        <button
          type="button"
          onClick={() => {
            setNote(null);
            void labPackMappingApply(draft)
              .then((saved) => {
                setDraft(saved);
                setNote("บันทึกร่างแล้ว (ไฟล์ dev ใต้โฟลเดอร์ข้อมูลแอป)");
              })
              .catch((reason: unknown) => setNote(String(reason)));
          }}
        >
          บันทึกร่าง
        </button>
        <button
          type="button"
          onClick={() => {
            void labPackMappingExport()
              .then(setExported)
              .catch((reason: unknown) => setNote(String(reason)));
          }}
        >
          Export mapping JSON
        </button>
        <button type="button" onClick={loadPack}>
          โหลด pack ใหม่
        </button>
      </div>
      {note && <p role="status">{note}</p>}
      {exported !== null && <textarea className="lab-export" readOnly value={exported} aria-label="Mapping JSON" />}

      <h4>เกมอาจมี แต่ยังไม่อยู่ในแคตตาล็อก (ยังไม่ได้ตรวจกับ payload จริง และยังไม่ได้สร้าง)</h4>
      <table className="lab-table">
        <thead>
          <tr>
            <th>id</th>
            <th>ชื่อ</th>
            <th>สถานะ</th>
            <th>หมายเหตุ</th>
          </tr>
        </thead>
        <tbody>
          {view.notInCatalog.map((item) => (
            <tr key={item.id}>
              <td>
                <code>{item.id}</code>
              </td>
              <td>{item.labelTh}</td>
              <td>{POSSIBLE_STATUS_TH[item.status]}</td>
              <td>{item.noteTh}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {view.gMaidenOnly.map((item) => (
        <p key={item.id} className="lab-note">
          มีเฉพาะ G-Maiden: <code>{item.id}</code> — {item.noteTh}
        </p>
      ))}
    </section>
  );
}
