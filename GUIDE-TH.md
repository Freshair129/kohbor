# คู่มือเอกสาร SWE — Kohbor / เกาะเบาะ

**เวอร์ชัน 0.3.3 · ฉบับร่าง · 11 พฤศจิกายน 2026**

ลิงก์ไปยังเอกสารอื่นในคู่มือนี้อ้างอิงโฟลเดอร์ภายใน repository หรือชุด ZIP เมื่อดาวน์โหลดหรือแตกไฟล์แล้วให้เปิด GUIDE-TH.md ที่อยู่ในโฟลเดอร์หลักของชุดเอกสาร

## ใช้ PRD และ SRS เป็นแกน

PRD กำหนดปัญหา ผู้ใช้ คุณค่าและขอบเขต ส่วน SRS แปลงเป็นพฤติกรรมระบบที่ตรวจรับได้ แล้วเชื่อมไปยังการออกแบบ การทดสอบและการส่งมอบ ชุดนี้มี **ความต้องการผลิตภัณฑ์ 12 ข้อ (PN-001–PN-012)** เชื่อมกับ **ข้อกำหนดระบบ 85 ข้อ** การเชื่อมครบช่วยให้ตรวจย้อนกลับได้ ไม่ใช่หลักฐานว่าแอปทำงานครบแล้ว

เริ่มจาก [สารบัญ](docs/INDEX.md):

| เอกสาร | หน้าที่ |
| --- | --- |
| [PRD](docs/product/PRD.md) | ทำเพื่อใคร แก้ปัญหาอะไร เป้าหมาย scope/priority และกฎผลิตภัณฑ์ |
| [Use Cases](docs/product/USE-CASES.md) | ผู้ใช้เริ่มอย่างไร ทำอะไร ทางเลือกและข้อผิดพลาด จบแล้วได้ผลใด |
| [SRS](docs/requirements/SRS.md) | ข้อกำหนดรายข้อ เงื่อนไข พฤติกรรมผิดพลาด และผลที่ตรวจรับได้ |
| [SAD](docs/design/SAD.md) / [SDD](docs/design/SDD.md) | Software Architecture Description: ขอบเขต/องค์ประกอบหลัก; Software Design Description: การทำงานภายใน สถานะและวงจรงาน |
| [DDD](docs/design/DDD.md) / [ICD](docs/design/ICD.md) | Data Design Description: entity/key/source/lifetime; Interface Control Document: request/response/event/auth/error/version |
| [UX Specification](docs/design/UX-SPEC.md) | หน้าจอ สถานะ การโต้ตอบ focus และ accessibility |
| [NFR](docs/quality/NFR.md) | ตัวเลขคุณภาพ ขอบเขตการวัด จำนวนตัวอย่าง และสภาพแวดล้อม |
| [Test Plan](docs/quality/TEST-PLAN.md) / [UAT Plan](docs/quality/UAT-PLAN.md) | วิธีทดสอบระบบ และตรวจว่าผู้ใช้ทำงานจริงสำเร็จ |
| [RTM](docs/requirements/RTM.csv) | เชื่อม PN → requirement → design/contract → test/evidence |
| [Operations Runbook](docs/operations/RUNBOOK.md) | ติดตั้ง ตั้งค่า diagnostics อัปเดต กู้คืนและ release |

[Security and Privacy](docs/security/SECURITY-PRIVACY.md), [Media Pack Specification](docs/design/MEDIA-PACK-SPEC.md) และ [Feature Catalog](docs/product/FEATURE-CATALOG.md) ลงรายละเอียดเฉพาะด้านโดยอ้างข้อกำหนดชุดเดียวกัน

## ไดอะแกรมที่อ่านร่วมกับสเปก

[หน้ารวมแผนภาพ](docs/diagrams/INDEX.html) มี 19 ภาพ: ขอบเขตระบบ องค์ประกอบ การติดตั้ง เส้นทางข้อมูล ERD ลำดับงาน สถานะ และเส้นทางใช้งาน อ่าน [สารบัญไดอะแกรม](docs/diagrams/README.md) เพื่อเลือกภาพตามคำถาม

แต่ละภาพมี ID, ต้นฉบับ Mermaid, SVG, เอกสารเจ้าของข้อกำหนด และรายการ SRS/decision ที่เกี่ยวข้อง ใช้ภาพเพื่อเห็นความสัมพันธ์และลำดับ พร้อมอ่านเงื่อนไขกำกับ เช่น ภาพการเตือนต้องไม่ตีความว่าคะแนนสูงอย่างเดียวให้อำนาจร้องเตือน หรือภาพข้อมูลต้องไม่เพิ่มสิทธิ์ส่งออกเอง

ไม่จำเป็นต้องวาดทุกชนิดของ UML ให้ครบจำนวน เลือกภาพที่ทำให้ขอบเขตและพฤติกรรมชัดขึ้น และแก้ต้นฉบับ/ภาพ/RTM ร่วมกันเมื่อข้อกำหนดเปลี่ยน

## ตัวอย่างการแยกหน้าที่

**PRD:** ผู้เล่นต้องการดูสถานะเพื่อนที่ยินยอมแชร์ และรู้ว่าข้อมูลยังสดหรือใช้ไม่ได้แล้ว

**SRS:** รับเฉพาะข้อมูลที่ผ่าน room/match/team admission; อายุรวมไม่เกิน 2,000 ms จึงแสดงว่าสด เมื่อเกินให้ถอน vitals ตามสัญญา ห้ามใช้ HP=0 แทนข้อมูลที่หาย

**DDD/ICD** กำหนด field, หน่วย, null และ clock domain; **SDD/UX** กำหนดการหมดอายุแม้ไม่มี IPC ใหม่; **Test/UAT** ตรวจ 2,000/2,001 ms, network ขาดและ session เปลี่ยน; **RTM** เชื่อม PN-009 กับ PEER-005 และหลักฐานที่เกี่ยวข้อง

คำว่า “แสดงแบบ real time” จึงยังไม่พอสำหรับลงมือสร้าง

## รูปแบบและแหล่งหลัก

Markdown UTF-8 เป็นเอกสารหลัก ส่วน [product-needs.json](contracts/product-needs.json), [requirements.json](contracts/requirements.json) และ [decisions.json](contracts/decisions.json) เก็บรายการที่ตรวจด้วยเครื่องได้ ใช้ [contracts](contracts/), [fixtures](fixtures/) และ [acceptance](acceptance/) สำหรับ schema ข้อมูลสังเคราะห์และสถานการณ์ทดสอบ

Registry ช่วยรักษาความสอดคล้อง แต่ไม่ทดแทนเหตุผลและการตัดสินใจใน PRD หากทำ PDF/DOCX สำหรับอ่านหรืออนุมัติ ควรสร้างจากแหล่งเดียวกัน ไม่แก้ข้อกำหนดแยกหลายฉบับ

## เรื่องที่ห้ามปล่อยให้เอเจนต์เดา

- **ข้อมูล:** unknown ไม่ใช่ zero/ready/safe; ภาพหายไม่พิสูจน์ว่าศัตรูหาย และคะแนน heuristic อย่างเดียวไม่ให้อำนาจร้องเตือน
- **Consent:** cloud advice, peer sharing, microphone, social presence และ recording แยกกัน การ sign-in หรือได้ premium ไม่เปิดสิทธิ์อื่นเอง
- **Scope:** เดโม event audio ไม่พิสูจน์ local conversation/memory; คลิปหลักฐานหนึ่งแมตช์ไม่พิสูจน์ automated highlights; local action-damage ยังรอมติขอบเขต MVP เต็ม
- **ทรัพยากร:** 900,000,000 bytes หมายถึง aggregate full Working Set ตามบัญชี process รวมโมเดลจริง ส่วน OBS ต้องวัดแยกและรายงานยอดรวม
- **UX:** production ไม่เติม mock; map ไม่ขยับเมื่อ inspect; การเสียสิทธิ์ premium ไม่หยุด local core
- **ความเป็นอิสระ:** core ใช้ได้โดยไม่มีบัญชี/เครือข่าย ส่วนที่ unavailable ต้องไม่ทำลายความสามารถอิสระที่ยังใช้ได้

## ชื่อกำหนดแล้ว อัตลักษณ์ทางเทคนิคยังเปิด

เจ้าของผลิตภัณฑ์เลือกชื่อ **Kohbor / เกาะเบาะ** แล้ว บันทึกอยู่ใน [product-identity.json](contracts/product-identity.json) เอเจนต์ยังออกแบบโลโก้ คาแรกเตอร์ สี ตัวอักษร และภาพประกอบต้นฉบับได้ตาม DEC-VISUALS โดยคงชื่อที่เลือกไว้

app/package ID, data directory, URI scheme, signature/hash namespace และ update origin ยังเป็นค่าที่ไม่กำหนด **DEC-NAME** จึงยังไม่ปิดทั้งรายการ เอเจนต์ต้องบันทึกและ freeze อัตลักษณ์ทางเทคนิคที่เกี่ยวข้องก่อนสร้าง application skeleton โดยประสานกับข้อกำหนด protocol/trust และไม่เดาโดเมนหรือบริการจากชื่อ การเลือกชื่อไม่ครอบคลุมการเปลี่ยน privacy, งบทรัพยากร, scope หรือการอนุมัติผลิตภัณฑ์ทั้งชุด

เอกสารยังเป็น **draft** มีประเด็นที่ต้องปิด เช่น full protocol บางส่วน, signature/canonicalization ของแพ็ก, mockup/assets จริง, model feasibility, patch/class coverage, retention และบริการ premium/social ดู [Decision Log](docs/governance/DECISION-LOG.md) ว่าใครเลือกได้ งานใดถูก block และปิดด้วยหลักฐานใด; [Risk Register](docs/governance/RISK-REGISTER.md) ระบุผลกระทบ

งานที่ไม่ขึ้นกับประเด็นเปิดทำต่อได้ ไม่ต้องหยุดทั้งโครงการ ขณะเดียวกันห้ามเปลี่ยนข้อไม่ทราบให้เป็นคำตอบที่แต่งขึ้น

## เอกสารทดสอบไม่ใช่ผลทดสอบ

หลักฐานแอปใหม่เริ่ม **NOT_RUN** การตรวจ JSON/schema/link/traceability/fixture พิสูจน์ความสอดคล้องของเอกสารเท่านั้น ต้องเก็บ Windows native, live Dota 2, audio timing และ CPU/RAM/FPS ตามแผนจริง พร้อม build/revision, environment, วิธีวัดและผลที่สังเกต Screenshot หรือ CI เขียวไม่แทนหลักฐานที่ยังไม่มี

ชุดนี้ไม่อ้างว่า “ครบจนไม่ต้องเดาเลย” หรือผ่านมาตรฐาน ISO/IEEE ทั้งฉบับ เป้าหมายคือแสดงสิ่งที่ต้องทำ สิ่งที่เลือกได้ และสิ่งที่ยังไม่ทราบให้ตรวจย้อนกลับได้

## แยก workflow ของเอเจนต์ออกจากข้อกำหนด

[AGENTS.md](AGENTS.md) และ [PROMPT.md](PROMPT.md) บอกวิธีทำงาน; [SDP — Software Development Plan](docs/delivery/SDP.md) จัด milestone/dependency/เกณฑ์ส่งมอบ โดยไม่แทน PRD หรือ SRS

1. อ่าน [Document Control](docs/governance/DOCUMENT-CONTROL.md), PRD, SRS และ Decision Log
2. ใช้ชื่อ Kohbor / เกาะเบาะ ยืนยัน scope และกำหนดอัตลักษณ์ทางเทคนิคที่ยังเปิด พร้อมปิดการตัดสินใจที่ได้รับมอบอำนาจ
3. จัด requirement-to-module/test mapping และ freeze สัญญาก่อนเชื่อมส่วนต่าง ๆ
4. สร้าง local-core slice กับสถานะ waiting/unknown แล้วเพิ่มงานตาม dependency
5. อัปเดต RTM และหลักฐานทุก milestone โดยไม่เรียกเดโมว่า MVP เต็ม

การเปลี่ยนข้อกำหนดต้องบันทึกตาม document control และ [Changelog](docs/governance/CHANGELOG.md) เพื่อให้เอเจนต์หรือผู้พัฒนารายถัดไปเข้าใจเหตุผลเดียวกัน

