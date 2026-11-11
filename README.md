# Kohbor / เกาะเบาะ

ชุดเอกสาร SWE สำหรับสร้าง Windows Desktop Companion สำหรับ Dota 2

**เวอร์ชัน 0.3.4 · 11 พฤศจิกายน 2026 · ฉบับร่าง · หลักฐานแอปใหม่: NOT_RUN**

ใช้ **PRD → SRS → เอกสารออกแบบ → แผนทดสอบและตรวจรับ** เป็นลำดับหลัก พร้อม RTM ที่เชื่อมความต้องการผลิตภัณฑ์ 12 ข้อกับข้อกำหนดระบบ 87 ข้อ เจ้าของผลิตภัณฑ์เลือกชื่อ **Kohbor / เกาะเบาะ** แล้ว เอเจนต์ออกแบบอัตลักษณ์ภาพใหม่ได้ โดยรักษาพฤติกรรม ขอบเขตข้อมูลและเกณฑ์ตรวจรับเดิม

บันทึกชื่ออยู่ใน [product-identity.json](contracts/product-identity.json) ส่วน app/package ID, data directory, URI scheme, signature/hash namespace และ update origin ยังไม่กำหนด การเลือกชื่อครั้งนี้จึงยังไม่ปิด **DEC-NAME** ทั้งรายการ เอเจนต์ต้องกำหนดและบันทึกอัตลักษณ์ทางเทคนิคก่อนสร้าง application skeleton โดยไม่เดาโดเมนหรือปลายทางบริการจากชื่อ

## เริ่มอ่าน

1. [PRD — Product Requirements Document](docs/product/PRD.md): ปัญหา ผู้ใช้ เป้าหมาย คุณค่า ขอบเขต และความต้องการผลิตภัณฑ์
2. [SRS — Software Requirements Specification](docs/requirements/SRS.md): บริบท ข้อจำกัด และข้อกำหนดทั้ง 87 ข้อพร้อมพฤติกรรมผิดพลาด/เกณฑ์ตรวจรับ
3. [สารบัญเอกสารวิศวกรรม](docs/INDEX.md): เลือก SAD, SDD, DDD, ICD, UX และข้อกำหนดเฉพาะด้านตามงาน
4. [Test Plan](docs/quality/TEST-PLAN.md), [UAT Plan](docs/quality/UAT-PLAN.md) และ [RTM](docs/requirements/RTM.csv): วิธีพิสูจน์และตรวจย้อนกลับ
5. [Document Control](docs/governance/DOCUMENT-CONTROL.md) และ [Decision Log](docs/governance/DECISION-LOG.md): อำนาจตัดสินใจ เวอร์ชัน และประเด็นที่ยังเปิด

คำอธิบายและตัวอย่างภาษาไทยอยู่ใน [GUIDE-TH.md](GUIDE-TH.md) สำหรับเริ่มงานเอเจนต์ ใช้ [AGENTS.md](AGENTS.md) และ [PROMPT.md](PROMPT.md) ร่วมกับ [Software Development Plan](docs/delivery/SDP.md)

## ไดอะแกรม

เปิด [หน้ารวมแผนภาพ](docs/diagrams/INDEX.html) เพื่อดู 19 ภาพ หรืออ่าน [สารบัญและวิธีแก้ไข](docs/diagrams/README.md) มีต้นฉบับ Mermaid และ SVG พร้อม requirement/decision references ภาพเป็นมุมมองของสเปกชุดเดียวกัน; เงื่อนไขเต็มยังอยู่ใน SRS และเอกสารออกแบบ

## เอกสารหลักกับข้อมูลประกอบ

PRD และ SRS เป็นข้อกำหนดที่คนอ่านและทบทวนได้ครบในระดับรายการที่บันทึกไว้ [requirements.json](contracts/requirements.json) เป็นข้อมูลโครงสร้างของข้อกำหนดชุดเดียวกัน ใช้สร้างส่วนรายละเอียดใน SRS, RTM และ Gherkin ไม่ใช่ข้อกำหนดอีกฉบับที่มีอำนาจเหนือเอกสารหลัก

ไฟล์ schema, fixture และเครื่องมือตรวจช่วยลดความคลาดเคลื่อนในการลงมือสร้าง ส่วนการออกแบบที่ยังไม่เลือกและข้อกำหนดที่ยังไม่สมบูรณ์อยู่ใน Decision Log ไม่ควรแปลงช่องว่างเหล่านั้นเป็นพฤติกรรมที่เดาขึ้นเอง

## ตรวจความสอดคล้อง

จากโฟลเดอร์ชุดเอกสาร:

```bash
python tools/render_requirements.py --check
python tools/validate_spec.py
```

หากแก้ข้อกำหนด ให้ปรับ registry และบริบท PRD/SRS ที่เกี่ยวข้อง แล้วรัน renderer โดยไม่ใส่ `--check` ก่อนตรวจอีกครั้ง ต้องมี Python package `jsonschema` เพื่อให้การตรวจ schema ครบ; หากไม่มี เครื่องมือรายงาน PARTIAL

ผลตรวจนี้ครอบคลุมเอกสาร ความเชื่อมโยงและข้อมูลสังเคราะห์เท่านั้น Windows native, live game, acoustic และ resource tests ยังต้องรันกับแอปจริง

## ความพร้อมของชุดนี้

เริ่มตรวจงานที่ยังต้องออกแบบและทดสอบต่อจาก [Implementation Readiness](docs/governance/IMPLEMENTATION-READINESS.md) ก่อนวางงานสร้างแอป

มีโครงเอกสารทางวิศวกรรม ความต้องการที่ตรวจย้อนกลับได้ พฤติกรรม/สถานะหลัก และแผนพิสูจน์ผลแล้ว ยังมีงานเปิด เช่น full protocol บางส่วน รูปแบบลายเซ็นแพ็ก แบบหน้าจอจริง โมเดล/ข้อมูลเกมที่ผ่านการวัด และบริการ optional ดู [Risk Register](docs/governance/RISK-REGISTER.md) และ [Changelog](docs/governance/CHANGELOG.md)

ชุดนี้ไม่อ้างความครบถ้วนทุกพฤติกรรมของระบบอ้างอิง ไม่อ้างว่าผ่านมาตรฐาน ISO/IEEE ทั้งฉบับ และไม่บันทึกการอนุมัติผลิตภัณฑ์หรือผลทดสอบที่ยังไม่เกิดขึ้น
