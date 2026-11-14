const messages = {
  th: {
    title: "บันทึกจากโฮสต์", level: "ระดับขั้นต่ำ", all: "ทั้งหมด", filter: "ค้นหา", filterPlaceholder: "กรองข้อความในบันทึก",
    pause: "หยุดอ่านชั่วคราว", resume: "อ่านต่อ", clear: "ล้างหน้าจอ", clearHint: "ล้างเฉพาะหน้าจอ ไม่ลบไฟล์บันทึก",
    copy: (count: number) => `คัดลอก ${count} บรรทัดล่าสุด`, count: "จำนวนบรรทัดที่คัดลอก", copyHint: "คัดลอกจากบรรทัดที่ผ่านตัวกรอง",
    copied: "คัดลอกแล้ว", manualCopy: "เลือกข้อความแล้ว — กด Ctrl+C เพื่อคัดลอก", manualLabel: "ข้อความสำหรับคัดลอก",
    open: "เปิดโฟลเดอร์บันทึก", folder: "โฟลเดอร์", openFailed: "เปิดโฟลเดอร์ไม่ได้ — ยังไม่มีบันทึกจากโฮสต์",
    loading: "กำลังอ่านบันทึก…", unavailable: "ยังไม่มีบันทึกจากโฮสต์", empty: "ยังไม่มีบันทึก", noMatch: "ไม่มีบรรทัดตรงกับตัวกรอง",
    dropped: "บางบรรทัดถูกข้าม", following: "ตามท้ายอยู่", stopped: "หยุดตามท้าย", follow: "ตามท้าย", paused: "พักการอ่าน",
    rows: (count: number) => `${count} บรรทัด`, retained: "แสดงบรรทัดล่าสุดที่ได้รับ",
  },
  en: {
    title: "Host logs", level: "Minimum level", all: "All", filter: "Search", filterPlaceholder: "Filter log text",
    pause: "Pause tail", resume: "Resume tail", clear: "Clear view", clearHint: "Clears this view only; log files are kept",
    copy: (count: number) => `Copy latest ${count} lines`, count: "Lines to copy", copyHint: "Copies lines matching the filters",
    copied: "Copied", manualCopy: "Text selected — press Ctrl+C to copy", manualLabel: "Text to copy",
    open: "Open log folder", folder: "Folder", openFailed: "Cannot open folder — no host logs available",
    loading: "Reading logs…", unavailable: "No host logs available", empty: "No logs yet", noMatch: "No matching lines",
    dropped: "Some lines were skipped", following: "Following tail", stopped: "Follow stopped", follow: "Follow tail", paused: "Tail paused",
    rows: (count: number) => `${count} lines`, retained: "Showing last received lines",
  },
};
export const logsCopy = (locale: "th" | "en") => messages[locale];
