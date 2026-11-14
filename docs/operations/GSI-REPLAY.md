# Recorded GSI Replay / เล่นซ้ำ GSI ที่บันทึกไว้

## ภาษาไทย

เครื่องมือ `tools/gsi_replay.py` ส่งข้อมูล GSI จาก session ที่บันทึกไว้ไปยัง Kohbor DEMO บนเครื่องเดียวกัน ใช้ทดสอบ Deck, hero cards, events และ announcer ได้โดยไม่ต้องเล่นแมตช์เต็ม 40 นาที ข้อมูลผู้เล่นและ Steam ID จะอยู่ในหน่วยความจำและส่งได้เฉพาะไปยัง DEMO ที่ loopback เท่านั้น เครื่องมือไม่เขียนข้อมูลที่ถอดรหัสแล้วหรือแก้ recording

หลักฐานจากการ replay จัดเป็น **synthetic/replay** เท่านั้น ช่วยลดเวลาตรวจสอบ แต่ไม่ใช่ live evidence และใช้แทนการยอมรับผลจาก live match ไม่ได้

1. เริ่ม Kohbor DEMO (บิลด์ debug หรือ `--features demo`; บิลด์ release ไม่รองรับ) โดยตั้ง `KOHBOR_DEMO=1`, `KOHBOR_DATA_DIR` เป็นโฟลเดอร์ใหม่ที่ว่างและเป็น path เต็ม และ `KOHBOR_TEST_TOKEN` เป็น token ชั่วคราว 16-128 ตัวอักษร (a-z A-Z 0-9 - _) ใช้ตัวแปรเดียวกันตอนรัน replay โฟลเดอร์ต้องเป็นโฟลเดอร์ใหม่ทุกครั้ง เพราะถ้ามีไฟล์ token เดิมอยู่แอปจะใช้ของเดิม ห้ามใช้ data directory จริง พอร์ตดูจากบรรทัด listener_bound ในหน้าบันทึก (log)
2. ดูรายการ session: `python tools/gsi_replay.py --list` หรือระบุ `--root F:\gsi-log`
3. replay ที่ 20 เท่า เริ่มที่เกมกำลังเล่น และระบุพอร์ตของ DEMO:

   ```powershell
   python tools/gsi_replay.py replay "F:\gsi-log\01-mid-20261103T143037Z-6f2d49ae" --port 3000 --demo --speed 20 --start-at phase:GAME_IN_PROGRESS
   ```

   ใช้ `--dry-run` เพื่อตรวจจำนวน record และเวลาที่จะ replay โดยไม่ส่งข้อมูล `--i-know-this-is-not-the-real-app` เป็น override ที่ไม่ปลอดภัยซึ่งข้ามการหยุดอัตโนมัติเมื่อได้ 401/403 เท่านั้น และยังต้องส่ง `--demo`; ห้ามใช้กับแอปจริง
4. ตรวจ Deck, ตัวเลขบน hero cards, event และ announcer ตามข้อมูล session ที่ส่งเข้า host โดย replay นี้ยืนยันเฉพาะสิ่งที่ app และ media build นั้นรองรับ
5. ขอบเขตข้อมูลบันทึก: ใช้ G-Maiden cfg ที่ขอ `provider`, `map`, `player`, `hero`, `abilities`, `items` แต่ไม่มี `allplayers` หรือ draft ดังนั้นมีข้อมูล hero card numbers; items/abilities ใช้ได้เมื่อ blocks ใน recording ตรงกับ own-snapshot 2.2.0 หลัง host รองรับแล้ว ส่วน draft bar ไม่มีข้อมูล, capture/minimap ไม่มีภาพหน้าจอ และ enemy/map vision ใช้ยืนยันไม่ได้

อนาคต (ยังไม่ได้สร้าง): สังเคราะห์ GSI แยกผู้เล่นจาก `.dem` ได้หลังเปรียบเทียบทุก field กับ recording แล้วเท่านั้น ดูการตรวจเดิมที่ `F:\gsi-log\_checks\compare_dem_gsi.py`

## English

`tools/gsi_replay.py` sends a recorded GSI session to a same-machine Kohbor DEMO. It can exercise the Deck, hero cards, events, and announcer without playing a full 40-minute match. Player and Steam ID fields remain in memory and are sent only to the loopback DEMO. The tool never writes decoded bodies or changes recordings.

Replay results are **synthetic/replay** evidence. They can shorten checks, but they are never live evidence and cannot replace live-match acceptance.

1. Start a DEMO instance (debug build or `--features demo`; release builds ignore the override) with `KOHBOR_DEMO=1`, `KOHBOR_DATA_DIR` set to a NEW empty absolute folder, and `KOHBOR_TEST_TOKEN` set to a temporary 16-128 character token (a-z A-Z 0-9 - _). Use the same variable when running the replay. The folder must be fresh each time because an existing token file wins. Never use the real app data directory. Find the port in the `listener_bound` log line.
2. List sessions with `python tools/gsi_replay.py --list`, or set an explicit root with `--root F:\gsi-log`.
3. Replay at 20x from the live-game phase using the DEMO port:

   ```powershell
   python tools/gsi_replay.py replay "F:\gsi-log\01-mid-20261103T143037Z-6f2d49ae" --port 3000 --demo --speed 20 --start-at phase:GAME_IN_PROGRESS
   ```

   Use `--dry-run` to inspect record count and planned duration without sending. `--i-know-this-is-not-the-real-app` is an unsafe override that only bypasses the automatic 401/403 stop; `--demo` is still required. Never use it with the real app.
4. Inspect the Deck, hero-card numbers, events, and announcer for the data delivered to the selected host. Replay only confirms behavior supported by that app and media build.
5. Recording limits: the G-Maiden cfg requested `provider`, `map`, `player`, `hero`, `abilities`, and `items`; it did not request `allplayers` or draft. Hero-card numbers are available. Items/abilities are usable if the recorded blocks match own-snapshot 2.2.0 after the host supports it. The draft bar has no data; capture/minimap has no screen image; enemy/map vision cannot be verified.

Future (not built): synthesize per-player GSI from `.dem` only after field-by-field comparison against recordings. See the existing check at `F:\gsi-log\_checks\compare_dem_gsi.py`.
