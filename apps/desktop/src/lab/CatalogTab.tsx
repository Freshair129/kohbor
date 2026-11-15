import { useEffect, useMemo, useState } from "react";
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
import { labCatalog } from "./labRuntime";

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
                <td className="lab-muted">ยังไม่มีข้อมูล (รอ host เปิดอ่าน pack ที่ใช้อยู่)</td>
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
