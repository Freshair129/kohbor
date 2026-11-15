---
title: "Progress Chase: ตั้งงานอัตโนมัติบน Windows (คู่มือสำหรับเจ้าของ)"
doc_id: "PROGRESS-CHASE-TH"
version: "0.1.0"
status: "draft"
updated: "2026-11-14"
owner: "Governance lead (integration owner)"
approval_status: "not_approved"
---

# Progress Chase: ตั้งงานอัตโนมัติบน Windows

เอกสารนี้สำหรับ **เจ้าของ** เท่านั้น เอเจนต์ไม่ได้สร้างงานตั้งเวลาให้ และจะไม่สร้างเอง ขั้นตอนด้านล่างคุณเป็นคนทำเอง

## งานนี้ทำอะไร

ทุกวัน (และตอนเข้าสู่ระบบ) ให้เครื่องรัน 3 อย่างเรียงกัน:

1. `git fetch` เพื่อดึงข้อมูลล่าสุดจาก GitHub (อ่านอย่างเดียว ไม่เปลี่ยนโฟลเดอร์งาน ไม่ checkout ไม่ merge)
2. `progress_map.py --ref origin/main` สร้างหน้าความคืบหน้า `F:\kohbor-local\progress\progress-map.html` จากโค้ดบน `origin/main` ไม่ใช่จากโฟลเดอร์ที่กำลังแก้อยู่
3. `--chase` เขียนรายการตามงาน `F:\kohbor-local\progress\chase.md`: ข้อกำหนด P0/P1 ที่ยังไม่มีใครรับ, claim ที่หมดอายุเกิน 24 ชั่วโมง, ปัญหา tag, รายงาน janitor ล่าสุด และเรื่องที่ต้องให้คุณตัดสินใจ

งานนี้ใช้ CPU ไม่กี่วินาที ไม่เขียนอะไรในโฟลเดอร์โปรเจกต์ ใช้เน็ตเฉพาะ `git fetch` และไม่ต้องใช้รหัสผ่านหรือ key ใดๆ

## ก่อนเริ่ม

- ต้องมี Python และ git ในเครื่อง (ตรวจด้วย `python --version` และ `git --version` ใน PowerShell)
- ต้องมีไฟล์ `tools\progress_map.py` ในโฟลเดอร์ที่จะรัน: ใช้โฟลเดอร์หลัก `F:\kohbor` หลังจาก PR ของเครื่องมือนี้ merge แล้วและคุณ `git pull` ในโฟลเดอร์นั้น (ถ้ายัง ให้ชี้ไปที่ worktree ที่มีไฟล์นี้ชั่วคราว)
- โฟลเดอร์ `F:\kohbor-local\progress` มีอยู่แล้ว (ถ้าไม่มี สคริปต์จะสร้างให้)

## วิธีที่ 1: ใช้หน้าจอ Task Scheduler

1. กด Start พิมพ์ `Task Scheduler` แล้วเปิด
2. ด้านขวากด **Create Task...** (ไม่ใช่ Create Basic Task)
3. แท็บ **General**: ตั้งชื่อ `Kohbor progress chase` เลือก **Run only when user is logged on** (จะได้ไม่ต้องเก็บรหัสผ่าน) และ **ไม่ต้อง** ติ๊ก Run with highest privileges
4. แท็บ **Triggers** กด **New...** สองครั้ง:
   - **Daily** เวลา 09:00 (ปรับได้)
   - **At log on** ของผู้ใช้ของคุณ
5. แท็บ **Actions** กด **New...**:
   - Program/script: `powershell.exe`
   - Add arguments: `-NoProfile -ExecutionPolicy Bypass -Command "git -C F:\kohbor fetch origin; python F:\kohbor\tools\progress_map.py --ref origin/main --chase"`
   - Start in: `F:\kohbor`
6. แท็บ **Conditions**: เอาเครื่องหมายออกจาก "Start the task only if the computer is on AC power" ถ้าใช้โน้ตบุ๊ก (ไม่งั้นงานจะไม่รันตอนใช้แบต)
7. แท็บ **Settings**: ติ๊ก **Run task as soon as possible after a scheduled start is missed** และตั้ง **Stop the task if it runs longer than** เป็น `10 minutes`
8. กด OK

## วิธีที่ 2: คำสั่งเดียว (คุณเป็นคนรันเอง; ยังไม่ได้ทดสอบ)

เปิด PowerShell ธรรมดา (ไม่ต้อง Administrator) แล้วรัน:

```powershell
schtasks /Create /TN "Kohbor progress chase" /SC DAILY /ST 09:00 /TR 'powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "git -C F:\kohbor fetch origin; python F:\kohbor\tools\progress_map.py --ref origin/main --chase"'
```

คำสั่งนี้ **ยังไม่ได้ทดสอบ (UNVERIFIED)**: การใส่เครื่องหมายคำพูดซ้อนกันระหว่าง PowerShell กับ `schtasks` มักพลาดได้ ถ้าขึ้น error หรืองานรันแล้วไม่เกิดไฟล์ ให้ลบงานแล้วใช้วิธีที่ 1 แทน คำสั่งนี้สร้างเฉพาะตัวรายวัน ถ้าต้องการ "ตอนเข้าสู่ระบบ" ด้วย ให้เพิ่มทริกเกอร์ในหน้าจอ Task Scheduler (วิธีที่ 1 ข้อ 4)

## ทดสอบว่าใช้ได้

1. ใน Task Scheduler คลิกขวาที่งาน แล้วเลือก **Run**
2. ดูไฟล์ `F:\kohbor-local\progress\chase.md` ว่าเวลาในบรรทัดแรก ("generated ...") เป็นปัจจุบัน
3. ถ้าไม่ขึ้น ให้เปิด PowerShell แล้วรันสองคำสั่งในข้อ 5 ของวิธีที่ 1 ทีละคำสั่งด้วยมือ ข้อความ error จะบอกว่าขาดอะไร (เช่น ไม่มี python ใน PATH หรือไม่มี `tools\progress_map.py` ในโฟลเดอร์นั้น)

## ปิด หยุด หรือลบ

- หยุดชั่วคราว: คลิกขวาที่งาน เลือก **Disable**
- ลบ: คลิกขวา เลือก **Delete** หรือรัน `schtasks /Delete /TN "Kohbor progress chase" /F`

## ข้อควรรู้

- ถ้าคุณเล่นเกมอยู่ งานนี้เบามาก แต่ถ้าไม่อยากให้รันตอนนั้น ให้เปลี่ยนเวลา Daily ไปช่วงที่ไม่เล่น
- ไฟล์ `chase.md` เป็นคำแนะนำ ไม่ได้สั่งให้ใครทำอะไร และหน้า progress map ขึ้นป้าย STALE เองเมื่อเก่าเกิน 6 ชั่วโมง
- ถ้า `git fetch` ไม่ผ่าน (ไม่มีเน็ต) งานจะยังสร้างหน้าจากข้อมูลเดิมที่มีในเครื่องได้ แต่ตัวเลขจะเก่า
- ต้องการเปลี่ยนที่เก็บไฟล์ ใช้ `--out` และ `--chase-out` ต่อท้ายคำสั่ง
