import { useEffect, useRef, useState } from "react";
import { commandErrorText, draftColumnApi, draftHostAvailable } from "./draftColumnApi";
import { bracketLabel, parseDraftStatsPrefs, retryBudgetView, type DraftColumnView as DraftColumnViewModel, type DraftStatsPrefs } from "./draftColumnModel";
import { createDraftViewRequestSequence, startDraftViewPolling } from "./draftViewPoller";
import "./draft-column.css";

function settingError(error: unknown): string {
  return commandErrorText(error);
}

export function DraftSettings() {
  const [prefs, setPrefs] = useState<DraftStatsPrefs | null>(null);
  const [view, setView] = useState<DraftColumnViewModel | null>(null);
  const [prefsError, setPrefsError] = useState<string | null>(null);
  const [viewError, setViewError] = useState<string | null>(null);
  const [retryError, setRetryError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const viewRequests = useRef(createDraftViewRequestSequence()).current;

  const refreshPrefs = async () => {
    try {
      const parsed = parseDraftStatsPrefs(await draftColumnApi.getPrefs());
      if (!parsed) throw new Error("draft_stats_prefs_get: invalid reply");
      setPrefs(parsed);
      setPrefsError(null);
    } catch (error) {
      setPrefs(null);
      setPrefsError(settingError(error));
    }
  };

  const refreshView = async () => {
    const request = viewRequests.next();
    try {
      const next = await draftColumnApi.getView();
      if (!viewRequests.isCurrent(request)) return;
      setView(next);
      setViewError(null);
    } catch (error) {
      if (!viewRequests.isCurrent(request)) return;
      setView(null);
      setViewError(settingError(error));
    }
  };

  useEffect(() => {
    void refreshPrefs();
    if (!draftHostAvailable()) {
      void refreshView();
      return;
    }
    return startDraftViewPolling(
      draftColumnApi.getView,
      (next) => {
        setView(next);
        setViewError(null);
      },
      (error) => {
        setView(null);
        setViewError(settingError(error));
      },
      undefined,
      viewRequests,
    );
  }, []);

  const update = async (patch: Partial<DraftStatsPrefs>) => {
    setBusy(true);
    setPrefsError(null);
    try {
      await draftColumnApi.setPrefs(patch);
      const parsed = parseDraftStatsPrefs(await draftColumnApi.getPrefs());
      if (!parsed) throw new Error("draft_stats_prefs_get: invalid reply");
      setPrefs(parsed);
      await refreshView();
    } catch (error) {
      setPrefsError(settingError(error));
    } finally {
      setBusy(false);
    }
  };

  const retry = async () => {
    setBusy(true);
    setRetryError(null);
    try {
      await draftColumnApi.retry();
      await refreshView();
    } catch (error) {
      setRetryError(settingError(error));
    } finally {
      setBusy(false);
    }
  };

  const retryBudget = retryBudgetView(view);

  return (
    <section className="draft-settings card" aria-labelledby="draft-settings-heading">
      <h2 id="draft-settings-heading">สถิติฮีโร่สาธารณะ</h2>
      <p className="draft-settings-purpose">
        ใช้แสดงสถิติย้อนหลังของฮีโร่ตามตำแหน่งและแรงก์ที่คุณเลือกก่อนเลือกฮีโร่ และแสดงสถิติของฮีโร่ที่คุณเลือกหลังเลือกแล้ว
      </p>

      <label className="draft-settings-switch">
        <input type="checkbox" role="switch" checked={prefs?.hero_stats_consent ?? false} disabled={!prefs || busy}
          onChange={(event) => { void update({ hero_stats_consent: event.currentTarget.checked }); }} />
        <span>ดึงสถิติฮีโร่จากอินเทอร์เน็ต</span>
      </label>
      <p className="draft-settings-note">ปิดเป็นค่าเริ่มต้น และมีเพียงความยินยอมสวิตช์นี้ที่เปิดคำขอสถิติ</p>
      <div className="draft-settings-consent-copy">
        <p>ผู้ให้บริการ: OpenDota · คำขอทั้งหมดใช้ HTTPS และส่งจากโฮสต์เท่านั้น</p>
        <ul>
          <li>ตารางฮีโร่สาธารณะ: GET https://api.opendota.com/api/heroStats ไม่มีข้อมูลของคุณหรือข้อมูลแมตช์</li>
          <li>ตารางบทบาทเลน: GET https://api.opendota.com/api/scenarios/laneRoles?lane_role=1, GET https://api.opendota.com/api/scenarios/laneRoles?lane_role=2 และ GET https://api.opendota.com/api/scenarios/laneRoles?lane_role=3 โดยแต่ละคำขอใช้ค่า lane_role คงที่</li>
          <li>สถิติความยาวเกมของฮีโร่ที่เลือก: GET https://api.opendota.com/api/heroes/{"{hero_id}"}/durations เรียกเมื่อโฮสต์รู้ฮีโร่ของคุณและไม่มีข้อมูลแคชใหม่ ใช้เฉพาะ ID ฮีโร่ของคุณ ไม่ส่งแรงก์ จำกัดไม่เกิน 30 คำขอต่อชั่วโมงและ 200 คำขอต่อช่วง 24 ชั่วโมงแบบเลื่อน</li>
        </ul>
        <p>ดึงตารางสาธารณะเมื่อเปิดแอปโดยเปิดความยินยอม เมื่อเปิดสวิตช์นี้ หรือเมื่อกดลองใหม่ ไม่ดึงตอนเริ่มดราฟต์ จำกัดคำขอตารางรวมไม่เกิน 4 ครั้งต่อช่วง 24 ชั่วโมงแบบเลื่อน โดยบันทึกงบไว้ข้ามการปิดเปิดแอป และไม่ดึงซ้ำเมื่อมีชุดข้อมูลครบที่ใหม่กว่า 24 ชั่วโมง</p>
        <p>ส่งคำขอครั้งละหนึ่งรายการ ใช้ HTTPS ไปที่ api.opendota.com:443 เท่านั้น โดยปิด redirect และ cookies ใช้ User-Agent ทั่วไป Kohbor จำกัดเวลาต่อเชื่อมต่อ 4 วินาที เวลารวม 10 วินาที และขนาดข้อมูลตอบกลับ 4 MiB; เมื่อได้ HTTP 429 จะรอ Retry-After ที่ผู้ให้บริการแจ้ง หากไม่มีให้รอ 1 ชั่วโมง และไม่ลองซ้ำอัตโนมัติ</p>
        <p>แคชในเครื่องเก็บตารางฮีโร่ ตารางเลนทั้งสาม และสถิติความยาวเกมของฮีโร่ที่เลือกได้ไม่เกิน 20 รายการ โดยนำรายการที่ไม่ได้ใช้นานที่สุดออกก่อน ข้อมูลอายุน้อยกว่า 24 ชั่วโมงถือว่าสด, ตั้งแต่ 24 ชั่วโมงถึงน้อยกว่า 7 วันแสดงเป็นข้อมูลเก่า, และอายุ 7 วันขึ้นไปจะถูกทิ้ง แคชไม่มีข้อมูลผู้เล่น แมตช์ หรือ Draft Bar และไม่ถูกเขียนลงล็อก เมื่อถอนความยินยอม ระบบหยุดคำขอใหม่ ยกเลิกคำขอที่กำลังทำ ทิ้งคำตอบที่มาช้า ล้างข้อมูลที่แสดง และลบแคช</p>
        <p>และดาวน์โหลดตารางสถิติรวมของฮีโร่ทุกตัว (ไม่มีข้อมูลของคุณในคำขอนี้)</p>
        <p>ผู้ให้บริการเห็น IP และเวลา</p>
      </div>

      <label className="draft-settings-switch">
        <input type="checkbox" role="switch" checked={prefs?.draft_bar_enabled ?? true} disabled={!prefs || busy}
          onChange={(event) => { void update({ draft_bar_enabled: event.currentTarget.checked }); }} />
        <span>แสดงแถบดราฟต์ในเกม</span>
      </label>
      <p className="draft-settings-note">การเปิดแถบดราฟต์ไม่เปิดความยินยอมดึงสถิติ</p>

      <div className="draft-settings-selects">
        <label>
          <span>ตำแหน่ง</span>
          <select value={prefs?.position ?? ""} disabled={!prefs || busy}
            onChange={(event) => { void update({ position: event.currentTarget.value === "" ? null : Number(event.currentTarget.value) }); }}>
            <option value="">ทุกตำแหน่ง</option>
            {[1, 2, 3, 4, 5].map((position) => <option key={position} value={position}>ตำแหน่ง {position} (ค่าประมาณ)</option>)}
          </select>
        </label>
        <label>
          <span>แรงก์</span>
          <select value={prefs?.bracket ?? ""} disabled={!prefs || busy}
            onChange={(event) => { void update({ bracket: event.currentTarget.value === "" ? null : Number(event.currentTarget.value) }); }}>
            <option value="">ทุกแรงก์</option>
            {Array.from({ length: 8 }, (_, index) => index + 1).map((bracket) => (
              <option key={bracket} value={bracket}>{bracketLabel(bracket)}</option>
            ))}
          </select>
        </label>
      </div>

      <div className="draft-settings-retry-row">
        <button type="button" disabled={!prefs || !prefs.hero_stats_consent || busy || retryBudget.used === null || retryBudget.spent}
          onClick={() => { void retry(); }}>
          ลองใหม่ · {retryBudget.label}
        </button>
        <span>ใช้คำขอ {retryBudget.label}</span>
      </div>
      {prefsError && <p className="draft-settings-error" role="alert">{prefsError}</p>}
      {viewError && <p className="draft-settings-error" role="alert">{viewError}</p>}
      {retryError && <p className="draft-settings-error" role="alert">{retryError}</p>}
      {!prefs && !prefsError && <p className="draft-settings-note" role="status">กำลังอ่านการตั้งค่าสถิติฮีโร่…</p>}
    </section>
  );
}
