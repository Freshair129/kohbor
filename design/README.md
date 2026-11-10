# Kohbor / เกาะเบาะ — Design Kit

**Design version 1.0.0 · 11 November 2026 · Requirements baseline 0.3.1 · Reviewable candidate**

ทิศทางภาพ **Nightglass**: พื้นเข้มแบบกระจกควัน สีหยกอ่อน ตัวอักษรไทยอ่านง่าย และสถานะข้อมูลที่สื่อความหมายตรงกันทั้งระบบ

## เริ่มจากไฟล์ไหน

| ต้องการ | เปิดไฟล์ |
| --- | --- |
| เห็นสี หน้าจอ คอมโพเนนต์ และสลับสถานะตัวอย่าง | [kohbor-design-board.html](board/kohbor-design-board.html) — ดาวน์โหลดแล้วเปิดในเบราว์เซอร์ได้แบบออฟไลน์ |
| ภาพรวมที่ดูหรือแชร์เป็นภาพได้ | [kohbor-design-board.png](board/kohbor-design-board.png) — ภาพออกแบบแบบคงที่ ไม่ใช่ screenshot ของแอป |
| กติกาออกแบบและตำแหน่งหน้าจอ | [DESIGN-SYSTEM.md](../docs/design/DESIGN-SYSTEM.md) |
| สัญญาการใช้งาน 24 คอมโพเนนต์ | [COMPONENT-CATALOG.md](../docs/design/COMPONENT-CATALOG.md) |
| ขอบเขตและวิธีใช้บอร์ด | [DESIGN-BOARD.md](../docs/design/DESIGN-BOARD.md) |
| โทเคนที่เอเจนต์และนักพัฒนานำไปใช้ | [Token README](tokens/README.md), [JSON](tokens/kohbor.tokens.json), [CSS](tokens/tokens.css) |
| ส่งงานให้เอเจนต์สร้าง UI | [IMPLEMENTATION-HANDOFF.md](../docs/design/IMPLEMENTATION-HANDOFF.md) |
| วิธีตรวจรับและผลที่ยังไม่ได้ทดสอบ | [DESIGN-ACCEPTANCE.md](../docs/design/DESIGN-ACCEPTANCE.md) |

HTML ไฟล์เดียวฝัง CSS, JavaScript, โทเคน, SVG และฟอนต์ไทย/อังกฤษพร้อมใบอนุญาตแล้ว ไม่ต้องติดตั้งแพ็กเกจเพื่อเปิดอ่านบอร์ด GitHub แสดง HTML เป็น source; ใช้ **Download raw file** แล้วเปิดไฟล์ที่ดาวน์โหลดมา

## ค่าที่ใช้จริง

- **248 tokens**: primitive 74, semantic 125, component 49; generated CSS 255 custom properties.
- **24 component contracts**: anatomy, states, actions, focus, data boundaries และการเชื่อมกลับไป SRS.
- **7 destinations** ตาม UX เดิม; Clips อยู่ใน Insights → History → Clips filter.
- Stage **1420 × 760**, panel **1280 × 720**, map **616 × 430**, right region **404 × 620**.
- Base/Pro ในบอร์ดเป็น fixture สำหรับดูหน้าตา สิทธิ์จริงยังต้องมาจาก native authority.

## แก้ไขและสร้างใหม่

แก้ token source ใน `tokens/source/` แล้วสร้างไฟล์ปลายทางใหม่ ไม่แก้ CSS ที่ generated โดยตรง:

```bash
python tools/build_design_tokens.py
python tools/validate_design.py
python tools/build_design_board.py
```

ตรวจ DOM logic ของบอร์ดด้วย dependency ที่ล็อกอยู่แล้วใน `tools/package-lock.json`:

```bash
npm ci --prefix tools --ignore-scripts --no-audit --no-fund
node tools/check_design_board.mjs
```

คำสั่งทั้งหมดรันจาก root ของรีโป ตัวสร้าง poster ใช้ Pillow กับ Raqm; ดูหัวไฟล์ [render_design_poster.py](../tools/render_design_poster.py) สำหรับขอบเขตการ render แบบคงที่

## สถานะการตรวจ

[Token receipt](tokens/token-validation.json): static token/type/reference/contrast/geometry checks. [Bundle receipt](board/bundle-validation.json): offline bundle/hash checks. [DOM receipt](board/dom-validation.json): synthetic DOM behavior checks; ไม่ใช่ผลจาก browser layout engine.

การตรวจภาพหน้าจอผ่านเบราว์เซอร์และการวัด viewport ยัง **NOT_RUN** ในรอบนี้ เนื่องจาก cloud browser เข้าถึง HTTP server ของ workspace ไม่ได้ และ URL policy ไม่อนุญาตให้อ่านหน้า error ที่ได้มา จึงหยุดเส้นทางนั้นโดยไม่เปลี่ยน browser surface หรือข้าม policy. Windows DPI, Thai shaping, focus, click-through, game safe areas, live data, audio และ resource/FPS ยัง **NOT_RUN** แยกต่างหาก

ชุดนี้ส่งมอบการออกแบบที่ตรวจทบทวนและใช้พัฒนาต่อได้ ไม่ใช่การรับรองแอปจริงหรือการอนุมัติปิด DEC-VISUALS ทุกส่วน
