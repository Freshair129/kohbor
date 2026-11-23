import type { AccountSnapshot } from "./accountView";

export type AccountLocale = "th" | "en";
export type AccountScreenId =
  | "A1" | "A1b" | "A1c" | "A2" | "A3" | "A4" | "A4e" | "A5" | "A5b" | "A6" | "A7" | "A7b" | "A7c" | "A8" | "A8b"
  | "E1" | "E2" | "E3" | "E5" | "E6" | "X1" | "X2";

export const ACCOUNT_COPY = {
  th: {
    "guest.title": "บัญชี (ไม่บังคับ)",
    "guest.body": "ทุกอย่างในเครื่องนี้ใช้ได้โดยไม่ต้องเข้าสู่ระบบ บัญชีมีไว้สำหรับเพื่อนและรายชื่อแชร์เท่านั้น",
    "signin.action": "เข้าสู่ระบบด้วย Google (เปิดเบราว์เซอร์)",
    // TODO(legal-review): confirm this Supabase account-data disclosure before shipping.
    "signin.promise": "การเข้าสู่ระบบเป็นทางเลือก ฟีเจอร์ในเครื่องยังใช้ได้แม้ไม่เข้าสู่ระบบ",
    // The disclosure wording in both locales is pending legal review.
    "signin.receives": "เกาะเบาะได้รับอีเมล ชื่อ และรูปโปรไฟล์ Google ของคุณผ่าน Supabase Auth และ Supabase จัดเก็บข้อมูลเหล่านี้เป็นข้อมูลบัญชี",
    "signin.alpha": "ช่วงทดสอบ: เข้าสู่ระบบได้เฉพาะบัญชี Google ที่ได้รับเชิญ",
    "guest.no_google": "ไม่มีบัญชี Google ก็ไม่เป็นไร ใช้ต่อแบบไม่เข้าสู่ระบบได้ครบทุกอย่างในเครื่อง",
    "signin.not_open": "บริการบัญชียังไม่เปิด ยังเข้าสู่ระบบไม่ได้ในรุ่นนี้",
    "signin.in_match": "คุณอยู่ในแมตช์ เข้าสู่ระบบได้หลังจบแมตช์ เราจะไม่เปิดเบราว์เซอร์ทับเกม",
    "action.in_match": "คุณอยู่ในแมตช์ จัดการบัญชีได้หลังจบแมตช์",
    "code.none": "รหัสเพื่อน: ยังไม่มี",
    "handoff.title": "ทำต่อในเบราว์เซอร์ให้เสร็จ",
    "handoff.step1": "เลือกบัญชี Google ในเบราว์เซอร์ที่เราเพิ่งเปิดให้",
    "handoff.step2": "กลับมาที่เกาะเบาะ หน้านี้จะไปต่อเอง",
    "handoff.waiting": "กำลังรอเบราว์เซอร์ · หมดเวลาใน {m:ss}",
    "handoff.cancel": "ยกเลิก",
    "handoff.reopen": "เปิดเบราว์เซอร์อีกครั้ง",
    "handoff.no_password": "เข้าสู่ระบบเฉพาะในเบราว์เซอร์ อย่าพิมพ์ข้อมูลบัญชีในแอปนี้",
    "handoff.cancelled": "ยกเลิกการเข้าสู่ระบบแล้ว ยังไม่มีอะไรเปลี่ยนแปลง",
    "checking.title": "กำลังตรวจสอบกับเกาะเบาะ",
    "checking.step1": "เลือกบัญชี Google แล้ว",
    "checking.step2": "กำลังเปิดบัญชีเกาะเบาะของคุณ ใช้เวลาไม่นาน",
    "checking.close_tab": "ปิดแท็บเบราว์เซอร์นั้นได้แล้ว",
    "name.title": "ตั้งชื่อที่เพื่อนจะเห็น",
    "name.not_google": "ชื่อที่แสดงมาจากโปรไฟล์ Google ของคุณ",
    "name.label": "ชื่อที่แสดง",
    "name.hint": "1 ถึง 32 ตัวอักษร เปลี่ยนได้ภายหลัง",
    "name.save": "บันทึกชื่อ",
    "name.who_sees": "คนที่เห็นชื่อนี้: เพื่อนของคุณ และคนที่คุณส่งหรือได้รับคำขอเป็นเพื่อนเท่านั้น",
    "name.err.empty": "ยังไม่ได้พิมพ์ชื่อ พิมพ์ชื่ออย่างน้อย 1 ตัวอักษร",
    "name.err.long": "ชื่อยาวเกิน 32 ตัวอักษร ลบออกบางส่วน",
    "name.err.refused": "ใช้ชื่อนี้ไม่ได้ ชื่อมีอักขระที่มองไม่เห็นหรือใช้ไม่ได้ พิมพ์ชื่อใหม่ด้วยตัวอักษร ตัวเลข หรือเว้นวรรค",
    "name.err.not_saved": "บันทึกชื่อไม่ได้ ชื่อที่พิมพ์ยังอยู่ ลองอีกครั้ง",
    "name.change": "เปลี่ยนชื่อ",
    "ready.title": "บัญชีพร้อมใช้แล้ว",
    "code.label": "รหัสเพื่อนของคุณ",
    "code.example": "ตัวอย่าง",
    "code.copy": "คัดลอก",
    "code.copied": "คัดลอกแล้ว",
    "code.explain": "ส่งรหัสนี้ให้คนที่คุณอยากเป็นเพื่อนด้วย ไม่มีรายชื่อผู้เล่นให้ค้นหา คนอื่นหาคุณได้ด้วยรหัสนี้เท่านั้น",
    "code.new": "ขอรหัสใหม่",
    "ready.done": "เสร็จแล้ว",
    "ready.nothing_shared": "ตอนนี้ยังไม่มีการแชร์อะไร การแชร์เริ่มเมื่อคุณเพิ่มเพื่อนเข้ารายชื่อแชร์เองเท่านั้น",
    "code.new.title": "ขอรหัสเพื่อนใหม่?",
    "code.new.c1": "รหัสเดิมจะใช้ไม่ได้ทันที ใครที่มีรหัสเดิมจะใช้เพิ่มคุณไม่ได้อีก",
    "code.new.c2": "เพื่อนของคุณยังอยู่ครบ รายชื่อแชร์ไม่เปลี่ยน คำขอเป็นเพื่อนที่รออยู่ยังอยู่เหมือนเดิม",
    "code.new.c3": "เปลี่ยนกลับเป็นรหัสเดิมไม่ได้ ขอรหัสใหม่ได้วันละ 3 ครั้ง",
    "code.new.limit": "วันนี้คุณขอรหัสใหม่ครบ 3 ครั้งแล้ว ลองใหม่พรุ่งนี้ รหัสปัจจุบันยังใช้ได้",
    "code.new.keep": "เก็บรหัสเดิม",
    "code.new.done": "ได้รหัสใหม่แล้ว รหัสเดิมใช้ไม่ได้แล้ว",
    "code.new.failed": "ขอรหัสใหม่ไม่ได้ รหัสเดิมยังใช้ได้ ลองใหม่ภายหลัง",
    "friend.request.failed": "ส่งคำขอไม่ได้ ตรวจรหัสอีกครั้งหรือลองใหม่ภายหลัง",
    "offline.title": "เข้าสู่ระบบอยู่ · ออฟไลน์",
    "offline.head": "เพื่อนและการแชร์หยุดชั่วคราว",
    "offline.body": "ทุกอย่างในเครื่องนี้ยังใช้ได้ตามปกติ ไม่มีอะไรถูกส่งหรือรับ",
    "offline.cached": "รหัสนี้เป็นค่าที่บันทึกไว้ในเครื่อง เมื่อกลับมาออนไลน์จึงจะขอรหัสใหม่ เปลี่ยนชื่อ ส่งออก หรือลบบัญชีได้",
    "offline.retry": "ลองเชื่อมต่ออีกครั้ง",
    "acct.online": "ออนไลน์",
    "acct.devices": "อุปกรณ์ที่เข้าสู่ระบบอยู่",
    "acct.devices.unknown": "ยังโหลดรายการอุปกรณ์ไม่ได้",
    "acct.devices.none": "ไม่มีอุปกรณ์อื่นในรายการนี้",
    "acct.this_pc": "เครื่องนี้",
    "acct.other_device": "อุปกรณ์อื่น",
    "acct.signed_in_on": "เข้าสู่ระบบเมื่อ {date}",
    "acct.signout": "ออกจากระบบ",
    "acct.signout.c": "ข้อมูลบัญชีที่บันทึกไว้ในเครื่องนี้จะถูกลบ บัญชีและเพื่อนของคุณยังอยู่ การตั้งค่าในเครื่องไม่เปลี่ยน",
    "acct.signout_all": "ออกจากระบบทุกเครื่อง",
    "acct.signout_all.title": "ออกจากระบบทุกเครื่อง?",
    "acct.signout_all.c1": "ทุกเครื่องรวมถึงเครื่องนี้จะออกจากระบบ ต้องเข้าสู่ระบบด้วย Google ใหม่",
    "acct.signout_all.c2": "ข้อมูลบัญชีที่บันทึกไว้ในเครื่องนี้จะถูกลบ: รายชื่อเพื่อนและรายชื่อแชร์",
    "acct.signout_all.c3": "บัญชี เพื่อน และรายชื่อแชร์บนบริการยังอยู่ครบ การตั้งค่าในเครื่องไม่เปลี่ยน",
    "acct.data": "ข้อมูลของคุณ",
    "export.action": "ส่งออกข้อมูลของฉัน",
    "export.body": "เกาะเบาะจะบันทึกไฟล์ JSON หนึ่งไฟล์ลงในที่ที่คุณเลือก ในไฟล์มี:",
    "export.i1": "ชื่อที่แสดงและรหัสเพื่อนของคุณ",
    "export.i2": "รายชื่อเพื่อน (ชื่อและรหัสเพื่อน)",
    "export.i3": "การตั้งค่ารายชื่อแชร์ และรายชื่อที่คุณบล็อก",
    "export.warn": "ไฟล์นี้มีชื่อและรหัสเพื่อนของคนอื่น เก็บไว้ในที่ปลอดภัย",
    "export.choose": "เลือกที่บันทึก",
    "export.done": "บันทึกแล้วที่ {path}",
    "export.failed": "บันทึกไฟล์ไม่ได้ เลือกโฟลเดอร์อื่น",
    "delete.action": "ลบบัญชี",
    "delete.title": "ลบบัญชีเกาะเบาะ",
    "delete.undo": "ลบแล้วกู้คืนไม่ได้",
    "delete.gone": "สิ่งที่จะถูกลบ",
    "delete.g1": "ชื่อที่แสดง รหัสเพื่อน รายชื่อเพื่อน รายชื่อแชร์ และรายชื่อที่บล็อก",
    "delete.g2": "การเข้าสู่ระบบบนทุกเครื่อง",
    "delete.g3": "เพื่อนจะเห็นคุณหายไปจากรายชื่อ โดยไม่มีคำอธิบาย",
    "delete.g4": "สำเนาสำรองที่เข้ารหัสจะถูกลบภายใน 30 วัน",
    "delete.stays": "สิ่งที่ยังอยู่",
    "delete.s1": "การตั้งค่า แพ็กเสียง และทุกฟีเจอร์ในเครื่องนี้",
    "delete.s2": "บัญชี Google ของคุณ เรายังไม่เปลี่ยนแปลงบัญชีนั้น",
    "delete.reauth": "เพื่อยืนยันว่าเป็นคุณ ให้เข้าสู่ระบบด้วย Google อีกครั้ง แล้วยืนยันการลบภายใน 5 นาที",
    "delete.keep": "เก็บบัญชีไว้",
    "delete.reauth.action": "เข้าสู่ระบบอีกครั้งเพื่อไปต่อ (เปิดเบราว์เซอร์)",
    "delete.confirm.title": "ยืนยันการลบบัญชี",
    "delete.confirm.window": "ยืนยันตัวตนแล้ว · ต้องยืนยันภายใน {m:ss}",
    "delete.confirm.body": "เมื่อกดปุ่มด้านล่าง บัญชีจะถูกลบทันทีและกู้คืนไม่ได้",
    "delete.final": "ลบบัญชีถาวร",
    "delete.done": "ลบบัญชีแล้ว คุณใช้งานต่อแบบไม่เข้าสู่ระบบได้ ทุกอย่างในเครื่องนี้ยังอยู่",
    "delete.failed": "ลบบัญชีไม่ได้ บัญชียังอยู่ครบ ลองใหม่ภายหลัง",
    "delete.err.mismatch": "บัญชี Google ที่เลือกไม่ใช่บัญชีที่ใช้กับบัญชีเกาะเบาะนี้ บัญชียังไม่ถูกลบ เข้าสู่ระบบอีกครั้งด้วยบัญชี Google เดิม",
    "delete.err.window": "หมดเวลายืนยันแล้ว บัญชียังไม่ถูกลบ",
    "delete.err.verify": "ยังยืนยันช่วงเวลาการเข้าสู่ระบบใหม่ไม่ได้ บัญชียังไม่ถูกลบ ลองเข้าสู่ระบบอีกครั้ง",
    "err.timeout.head": "การเข้าสู่ระบบหมดเวลา",
    "err.timeout.body": "เราไม่ได้รับคำตอบจากเบราว์เซอร์ อาจเพราะหน้าต่างถูกปิดไปก่อน ยังไม่มีอะไรเปลี่ยนแปลง ลองอีกครั้งได้เลย",
    "err.service.head": "ติดต่อบริการบัญชีไม่ได้ตอนนี้",
    "err.service.body": "ทุกอย่างในเครื่องนี้ยังใช้ได้ตามปกติ ลองใหม่ภายหลัง",
    "err.google.head": "Google ไม่ได้ยืนยันการเข้าสู่ระบบ",
    "err.google.body": "คุณอาจกดยกเลิกในเบราว์เซอร์ ยังไม่มีอะไรเปลี่ยนแปลง ลองอีกครั้งหรือเลือกบัญชี Google อื่น",
    "err.google.alpha": "…หรือบัญชี Google นี้ยังไม่ได้รับเชิญให้ร่วมทดสอบ",
    "err.rate.head": "คุณลองบ่อยเกินไป",
    "err.rate.body": "รอสักครู่แล้วลองใหม่",
    "err.rate.wait": "ลองได้อีกครั้งใน {m:ss}",
    "err.retry": "ลองอีกครั้ง",
    "err.later": "ไว้ทีหลัง",
    "revoked.head": "เครื่องนี้ออกจากระบบแล้ว",
    "revoked.body": "มีการออกจากระบบจากอุปกรณ์อื่น หรือการเข้าสู่ระบบหมดอายุ ข้อมูลบัญชีในเครื่องนี้ถูกลบแล้ว เข้าสู่ระบบอีกครั้งเมื่อพร้อม",
    "newdevice.head": "เข้าสู่ระบบบนเครื่องใหม่: การแชร์หยุดไว้ก่อน",
    "newdevice.body": "เพื่อความปลอดภัย รายชื่อแชร์ทุกรายการหยุดชั่วคราวจนกว่าคุณจะยืนยัน เพื่อนของคุณยังอยู่ครบ",
  },
  en: {
    "guest.title": "Account (optional)",
    "guest.body": "Everything on this PC works without signing in. An account is only for friends and the Share list.",
    "signin.action": "Sign in with Google (opens your browser)",
    "signin.promise": "Sign-in is optional; local features stay available if you skip it.",
    "signin.receives": "Kohbor receives your Google email, name, and profile picture through Supabase Auth. Supabase stores these account details.",
    "signin.alpha": "Test period: only invited Google accounts can sign in.",
    "guest.no_google": "No Google account? That is fine. Everything on this PC keeps working without one.",
    "signin.not_open": "The account service is not open yet. You cannot sign in in this version.",
    "signin.in_match": "You are in a match. You can sign in when it ends. We will not open a browser over your game.",
    "action.in_match": "You are in a match. Account changes are available when it ends.",
    "code.none": "Friend code: none yet",
    "handoff.title": "Finish signing in in your browser",
    "handoff.step1": "Choose your Google account in the browser we just opened.",
    "handoff.step2": "Come back to Kohbor. This page continues by itself.",
    "handoff.waiting": "Waiting for your browser · times out in {m:ss}",
    "handoff.cancel": "Cancel",
    "handoff.reopen": "Open the browser again",
    "handoff.no_password": "Complete sign-in only in your browser. Do not enter account details in this app.",
    "handoff.cancelled": "Sign-in cancelled. Nothing has changed.",
    "checking.title": "Checking with Kohbor",
    "checking.step1": "Google account chosen.",
    "checking.step2": "Opening your Kohbor account. This takes a moment.",
    "checking.close_tab": "You can close that browser tab now.",
    "name.title": "Choose the name your friends see",
    "name.not_google": "Your display name comes from your Google profile.",
    "name.label": "Display name",
    "name.hint": "1 to 32 characters. You can change it later.",
    "name.save": "Save name",
    "name.who_sees": "Who sees this name: your friends, and people you send a friend request to or get one from.",
    "name.err.empty": "No name yet. Type at least 1 character.",
    "name.err.long": "The name is longer than 32 characters. Remove some.",
    "name.err.refused": "This name cannot be used. It contains a hidden or unsupported character. Type it again using letters, numbers or spaces.",
    "name.err.not_saved": "The name was not saved. What you typed is still here. Try again.",
    "name.change": "Change name",
    "ready.title": "Your account is ready",
    "code.label": "Your friend code",
    "code.example": "Example",
    "code.copy": "Copy",
    "code.copied": "Copied",
    "code.explain": "Give this code to people you want as friends. There is no player directory: this code is the only way to find you.",
    "code.new": "Get a new code",
    "ready.done": "Done",
    "ready.nothing_shared": "Nothing is being shared. Sharing starts only when you add a friend to your Share list yourself.",
    "code.new.title": "Get a new friend code?",
    "code.new.c1": "The old code stops working at once. Anyone who has it can no longer use it to add you.",
    "code.new.c2": "Your friends stay. Your Share list does not change. Friend requests that are waiting stay as they are.",
    "code.new.c3": "You cannot go back to the old code. You can get a new code 3 times a day.",
    "code.new.limit": "You have used today's 3 new codes. Try again tomorrow. Your current code still works.",
    "code.new.keep": "Keep this code",
    "code.new.done": "You have a new code. The old one no longer works.",
    "code.new.failed": "Could not get a new code. Your current code still works. Try again later.",
    "friend.request.failed": "Could not send. Check the code or try later.",
    "offline.title": "Signed in, offline",
    "offline.head": "Friends and sharing are paused",
    "offline.body": "Everything on this PC keeps working. Nothing is sent or received.",
    "offline.cached": "This code is the copy saved on this PC. Getting a new code, changing your name, export and delete need a connection.",
    "offline.retry": "Try to connect again",
    "acct.online": "Online",
    "acct.devices": "Devices signed in",
    "acct.devices.unknown": "The device list is not available yet.",
    "acct.devices.none": "There are no other devices in this list.",
    "acct.this_pc": "This PC",
    "acct.other_device": "Another device",
    "acct.signed_in_on": "Signed in on {date}",
    "acct.signout": "Sign out",
    "acct.signout.c": "Account data saved on this PC is erased. Your account and friends stay. Settings on this PC do not change.",
    "acct.signout_all": "Sign out everywhere",
    "acct.signout_all.title": "Sign out everywhere?",
    "acct.signout_all.c1": "Every device, including this PC, is signed out. You sign in with Google again.",
    "acct.signout_all.c2": "Account data saved on this PC is erased: your friend list and Share list.",
    "acct.signout_all.c3": "Your account, friends and Share list stay on the service. Settings on this PC do not change.",
    "acct.data": "Your data",
    "export.action": "Export my data",
    "export.body": "Kohbor saves one JSON file where you choose. It contains:",
    "export.i1": "Your display name and friend code",
    "export.i2": "Your friend list (names and friend codes)",
    "export.i3": "Your Share list settings and block list",
    "export.warn": "The file holds other people's names and friend codes. Keep it somewhere safe.",
    "export.choose": "Choose where to save",
    "export.done": "Saved to {path}",
    "export.failed": "Could not save the file. Choose another folder.",
    "delete.action": "Delete account",
    "delete.title": "Delete your Kohbor account",
    "delete.undo": "This cannot be undone",
    "delete.gone": "What is deleted",
    "delete.g1": "Your display name, friend code, friend list, Share list and block list",
    "delete.g2": "Your sign-in on every device",
    "delete.g3": "Friends see you disappear from their list. No reason is shown.",
    "delete.g4": "Encrypted backups are deleted within 30 days.",
    "delete.stays": "What stays",
    "delete.s1": "Settings, sound packs and every feature on this PC",
    "delete.s2": "Your Google account. It is not changed.",
    "delete.reauth": "To confirm it is you, sign in with Google again, then confirm the deletion within 5 minutes.",
    "delete.keep": "Keep my account",
    "delete.reauth.action": "Sign in again to continue (opens your browser)",
    "delete.confirm.title": "Confirm account deletion",
    "delete.confirm.window": "Identity confirmed · confirm within {m:ss}",
    "delete.confirm.body": "When you press the button below, the account is deleted at once and cannot be restored.",
    "delete.final": "Delete account permanently",
    "delete.done": "Account deleted. You can keep using Kohbor without signing in. Everything on this PC is still here.",
    "delete.failed": "The account could not be deleted. It is unchanged. Try again later.",
    "delete.err.mismatch": "The Google account you chose is not the one this Kohbor account uses. Nothing was deleted. Sign in again with the same Google account.",
    "delete.err.window": "The time to confirm ran out. Nothing was deleted.",
    "delete.err.verify": "The fresh sign-in window could not be confirmed. Nothing was deleted. Sign in again to continue.",
    "err.timeout.head": "Sign-in timed out",
    "err.timeout.body": "We did not hear back from your browser. The window may have been closed. Nothing has changed. You can try again.",
    "err.service.head": "The account service cannot be reached right now",
    "err.service.body": "Everything on this PC keeps working. Try again later.",
    "err.google.head": "Google did not confirm the sign-in",
    "err.google.body": "You may have pressed Cancel in the browser. Nothing has changed. Try again or choose another Google account.",
    "err.google.alpha": "…or this Google account has not been invited to the test.",
    "err.rate.head": "Too many attempts",
    "err.rate.body": "Wait a moment, then try again.",
    "err.rate.wait": "You can try again in {m:ss}",
    "err.retry": "Try again",
    "err.later": "Not now",
    "revoked.head": "This PC was signed out",
    "revoked.body": "You were signed out from another device, or the sign-in expired. Account data on this PC was erased. Sign in again when you are ready.",
    "newdevice.head": "New device: sharing is paused",
    "newdevice.body": "To keep you safe, every Share list entry is paused until you confirm. Your friends are all still here.",
  },
} as const satisfies Record<AccountLocale, Record<string, string>>;

export const ACCOUNT_SCREEN_IDS: readonly AccountScreenId[] = [
  "A1", "A1b", "A1c", "A2", "A3", "A4", "A4e", "A5", "A5b", "A6", "A7", "A7b", "A7c", "A8", "A8b",
  "E1", "E2", "E3", "E5", "E6", "X1", "X2",
];

export const ACCOUNT_SCREEN_COPY_KEYS: Readonly<Record<AccountScreenId, readonly (keyof typeof ACCOUNT_COPY.en)[]>> = {
  A1: ["guest.title", "guest.body", "signin.action", "signin.promise", "signin.receives", "signin.alpha", "guest.no_google", "code.none"],
  A1b: ["guest.title", "guest.body", "signin.action", "signin.promise", "signin.receives", "signin.alpha", "signin.not_open", "guest.no_google", "code.none"],
  A1c: ["guest.title", "guest.body", "signin.action", "signin.promise", "signin.receives", "signin.alpha", "signin.in_match", "guest.no_google", "code.none"],
  A2: ["handoff.title", "handoff.step1", "handoff.step2", "handoff.waiting", "handoff.cancel", "handoff.reopen", "handoff.no_password"],
  A3: ["checking.title", "checking.step1", "checking.step2", "checking.close_tab"],
  A4: ["name.title", "name.not_google", "name.label", "name.hint", "name.save", "name.who_sees", "name.err.empty", "name.err.long", "name.err.not_saved"],
  A4e: ["name.title", "name.not_google", "name.label", "name.hint", "name.save", "name.who_sees", "name.err.refused"],
  A5: ["ready.title", "code.label", "code.example", "code.copy", "code.copied", "code.explain", "code.new", "ready.done", "ready.nothing_shared"],
  A5b: ["code.new.title", "code.new.c1", "code.new.c2", "code.new.c3", "code.new.limit", "code.new.keep", "code.new.done", "code.new.failed"],
  A6: ["offline.title", "offline.head", "offline.body", "offline.cached", "offline.retry", "code.label", "code.none"],
  A7: ["acct.online", "acct.devices", "acct.devices.unknown", "acct.devices.none", "acct.this_pc", "acct.other_device", "acct.signed_in_on", "acct.signout", "acct.signout.c", "acct.signout_all", "acct.data", "export.action", "export.body", "export.i1", "export.i2", "export.i3", "export.warn", "name.label", "name.change", "code.label", "code.example", "code.copy", "code.copied", "code.explain", "code.new"],
  A7b: ["acct.signout_all.title", "acct.signout_all.c1", "acct.signout_all.c2", "acct.signout_all.c3", "acct.signout_all", "err.later"],
  A7c: ["acct.data", "export.body", "export.i1", "export.i2", "export.i3", "export.warn", "export.choose", "export.done", "export.failed", "err.later"],
  A8: ["delete.title", "delete.undo", "delete.gone", "delete.g1", "delete.g2", "delete.g3", "delete.g4", "delete.stays", "delete.s1", "delete.s2", "delete.reauth", "delete.keep", "delete.reauth.action", "handoff.waiting", "handoff.cancel", "handoff.reopen"],
  A8b: ["delete.confirm.title", "delete.confirm.window", "delete.confirm.body", "delete.final", "delete.keep", "delete.err.window", "delete.err.verify"],
  E1: ["err.timeout.head", "err.timeout.body", "err.retry"],
  E2: ["err.service.head", "err.service.body", "err.retry"],
  E3: ["err.google.head", "err.google.body", "err.google.alpha", "err.retry"],
  E5: ["err.rate.head", "err.rate.body", "err.rate.wait", "err.retry"],
  E6: ["delete.err.mismatch", "delete.err.window", "err.retry", "err.later"],
  X1: ["revoked.head", "revoked.body", "signin.action"],
  X2: ["newdevice.head", "newdevice.body"],
};

type AccountViewState = AccountSnapshot["state"];
export type AccountViewModel = {
  locale: AccountLocale;
  screenId: AccountScreenId;
  invitedOnly: boolean;
  handoffPurpose: "sign_in" | "delete";
  title: string;
  copyKeys: readonly (keyof typeof ACCOUNT_COPY.en)[];
  copy: Readonly<Record<string, string>>;
  announcementRole: "status" | "alert";
  tone: "plain" | "notice" | "error";
  identity: { displayName: string | null; displayNameLabel: string | null };
  friendCode: { value: string | null; presentation: "live" | "cached" | "unknown"; className: string; label: string; example: boolean };
  graphemeCount: number;
  handoffSecondsRemaining: number | null;
  rateLimitSecondsRemaining: number | null;
  deleteSecondsRemaining: number | null;
  sessions: readonly { thisDevice: boolean; signedInOn: string | null }[] | null;
  actions: {
    signIn: { disabled: boolean; reason: string | null };
    cancelHandoff: { disabled: boolean };
    saveName: { disabled: boolean; reason: string | null };
    replaceCode: { disabled: boolean; reason: string | null };
    finishSetup: { disabled: boolean };
    signOutHere: { disabled: boolean };
    signOutEverywhere: { disabled: boolean };
    export: { disabled: boolean };
    deleteAccount: { disabled: boolean; reason: string | null };
    continueDelete: { disabled: boolean };
    confirmDelete: { disabled: boolean; reason: string | null };
    keepAccount: { disabled: boolean };
  };
  friendRequestFailure: string | null;
};

const titles: Readonly<Record<AccountScreenId, keyof typeof ACCOUNT_COPY.en>> = {
  A1: "guest.title", A1b: "guest.title", A1c: "guest.title", A2: "handoff.title", A3: "checking.title",
  A4: "name.title", A4e: "name.title", A5: "ready.title", A5b: "code.new.title", A6: "offline.title",
  A7: "acct.online", A7b: "acct.signout_all.title", A7c: "acct.data", A8: "delete.title", A8b: "delete.confirm.title",
  E1: "err.timeout.head", E2: "err.service.head", E3: "err.google.head", E5: "err.rate.head", E6: "delete.err.mismatch",
  X1: "revoked.head", X2: "newdevice.head",
};

export function countGraphemeClusters(value: string, locale = "th"): number {
  return [...new Intl.Segmenter(locale, { granularity: "grapheme" }).segment(value)].length;
}

export function accountCopy(locale: AccountLocale, key: keyof typeof ACCOUNT_COPY.en, values: Record<string, string | number> = {}): string {
  const template = ACCOUNT_COPY[locale][key] as string;
  return template.replace(/\{([^}]+)\}/g, (match, name: string) => String(values[name] ?? match));
}

export function friendRequestFailureCopy(locale: AccountLocale, _reason?: string): string {
  return accountCopy(locale, "friend.request.failed");
}

export function accountControlStatus(snapshot: AccountSnapshot, locale: AccountLocale): string {
  const key: keyof typeof ACCOUNT_COPY.en = snapshot.state === "signed_out" ? "guest.title"
    : snapshot.state === "browser_handoff" ? "handoff.title"
      : snapshot.state === "signing_in" ? "checking.title"
        : snapshot.state === "needs_display_name" ? "name.title"
          : snapshot.state === "signed_in_online" ? "acct.online"
            : snapshot.state === "signed_in_offline" ? "offline.title"
              : snapshot.state === "service_unavailable" ? "err.service.head"
                : snapshot.state === "session_revoked" ? "revoked.head"
                  : snapshot.state === "paused_new_device" ? "newdevice.head" : "delete.title";
  return accountCopy(locale, key);
}

export function resolveAccountScreen(snapshot: AccountSnapshot, now = Date.now()): AccountScreenId {
  if (snapshot.state === "browser_handoff" && snapshot.handoffDeadline !== null && now >= snapshot.handoffDeadline) {
    return snapshot.handoffPurpose === "delete" ? "E6" : "E1";
  }
  if (snapshot.state === "deleting" && snapshot.deleteStep === "confirm" && snapshot.freshSignInDeadline !== null && now >= snapshot.freshSignInDeadline) return "E6";
  if (snapshot.deleteError === "mismatch" || snapshot.deleteError === "window") return "E6";
  if (snapshot.rateLimitDeadline !== null && now < snapshot.rateLimitDeadline) return "E5";
  if (snapshot.state === "signed_out") {
    if (snapshot.notice === "provider_declined") return "E3";
    if (snapshot.liveMatch) return "A1c";
    return snapshot.serviceOpen ? "A1" : "A1b";
  }
  if (snapshot.state === "browser_handoff") return snapshot.handoffPurpose === "delete" ? "A8" : "A2";
  if (snapshot.state === "signing_in") return "A3";
  if (snapshot.state === "needs_display_name") return snapshot.nameError === "refused" ? "A4e" : "A4";
  if (snapshot.state === "signed_in_online") {
    if (snapshot.dialog === "replace_code") return "A5b";
    if (snapshot.dialog === "sign_out_everywhere") return "A7b";
    if (snapshot.dialog === "export") return "A7c";
    return snapshot.firstSignIn ? "A5" : "A7";
  }
  if (snapshot.state === "signed_in_offline") return "A6";
  if (snapshot.state === "service_unavailable") return "E2";
  if (snapshot.state === "session_revoked") return "X1";
  if (snapshot.state === "paused_new_device") return "X2";
  if (snapshot.state === "deleting") {
    if (snapshot.deleteError === "failed") return "A8";
    return snapshot.deleteStep === "confirm" ? "A8b" : "A8";
  }
  return assertNever(snapshot.state);
}

export function buildAccountViewModel(snapshot: AccountSnapshot, locale: AccountLocale, now = Date.now()): AccountViewModel {
  const screenId = resolveAccountScreen(snapshot, now);
  const copyKeys = ACCOUNT_SCREEN_COPY_KEYS[screenId];
  const copy = Object.fromEntries(copyKeys.map((key) => [key, accountCopy(locale, key)]));
  const title = accountCopy(locale, titles[screenId]);
  const graphemeCount = snapshot.displayName === null ? 0 : countGraphemeClusters(snapshot.displayName, locale);
  const signInUnavailable = !snapshot.serviceOpen || snapshot.liveMatch;
  const signInReason = snapshot.liveMatch ? accountCopy(locale, "signin.in_match")
    : !snapshot.serviceOpen ? accountCopy(locale, "signin.not_open") : null;
  const friendCodePresentation = snapshot.friendCode.value === null ? "unknown"
    : snapshot.friendCode.freshness === "cached" || snapshot.state === "signed_in_offline" ? "cached"
      : snapshot.friendCode.freshness === "live" && snapshot.state === "signed_in_online" ? "live" : "unknown";
  const friendCodeValue = friendCodePresentation === "unknown" ? null : snapshot.friendCode.value;
  const friendCodeClassName = friendCodePresentation === "live" ? "account-code is-live"
    : friendCodePresentation === "cached" ? "account-code is-cached" : "account-code is-none";
  const deleteDisabled = snapshot.liveMatch || snapshot.state !== "signed_in_online";
  const deleteReason = snapshot.liveMatch ? accountCopy(locale, "action.in_match") : null;
  const handoffSecondsRemaining = snapshot.handoffDeadline === null ? null : Math.max(0, Math.ceil((snapshot.handoffDeadline - now) / 1000));
  const rateLimitSecondsRemaining = snapshot.rateLimitDeadline === null ? null : Math.max(0, Math.ceil((snapshot.rateLimitDeadline - now) / 1000));
  const deleteSecondsRemaining = snapshot.freshSignInDeadline === null ? null : Math.max(0, Math.ceil((snapshot.freshSignInDeadline - now) / 1000));
  const statusScreens: readonly AccountScreenId[] = ["A2", "A3", "A6", "E2", "X1", "X2"];
  const alertScreens: readonly AccountScreenId[] = ["A4e", "E1", "E3", "E5", "E6"];
  const tone = alertScreens.includes(screenId) ? "error" : statusScreens.includes(screenId) ? "notice" : "plain";
  const sessionRows = snapshot.sessions?.map((session) => ({ thisDevice: session.thisDevice, signedInOn: session.signedInOn })) ?? null;
  return {
    locale,
    screenId,
    invitedOnly: snapshot.invitedOnly,
    handoffPurpose: snapshot.handoffPurpose,
    title,
    copyKeys,
    copy,
    announcementRole: alertScreens.includes(screenId) ? "alert" : "status",
    tone,
    identity: {
      displayName: snapshot.displayName,
      displayNameLabel: snapshot.displayName === null ? null : accountCopy(locale, "name.label"),
    },
    friendCode: {
      value: friendCodeValue,
      presentation: friendCodePresentation,
      className: friendCodeClassName,
      label: accountCopy(locale, "code.label"),
      example: snapshot.friendCode.isExample === true,
    },
    graphemeCount,
    handoffSecondsRemaining,
    rateLimitSecondsRemaining,
    deleteSecondsRemaining,
    sessions: sessionRows,
    actions: {
      signIn: { disabled: signInUnavailable || screenId === "E5", reason: screenId === "E5" ? accountCopy(locale, "err.rate.body") : signInReason },
      cancelHandoff: { disabled: screenId !== "A2" },
      saveName: { disabled: snapshot.state !== "needs_display_name", reason: null },
      replaceCode: { disabled: snapshot.state !== "signed_in_online" || (snapshot.codeReplacementsRemaining !== null && snapshot.codeReplacementsRemaining <= 0), reason: snapshot.codeReplacementsRemaining === 0 ? accountCopy(locale, "code.new.limit") : null },
      finishSetup: { disabled: screenId !== "A5" },
      signOutHere: { disabled: !["signed_in_online", "signed_in_offline"].includes(snapshot.state) },
      signOutEverywhere: { disabled: snapshot.state !== "signed_in_online" },
      export: { disabled: snapshot.state !== "signed_in_online" },
      deleteAccount: { disabled: deleteDisabled, reason: deleteReason },
      continueDelete: {
        disabled: snapshot.liveMatch || !(
          (snapshot.state === "deleting" && ["review", "reauth"].includes(snapshot.deleteStep ?? ""))
          || (snapshot.state === "browser_handoff" && snapshot.handoffPurpose === "delete" && snapshot.deleteStep === "reauth")
        ),
      },
      confirmDelete: {
        disabled: snapshot.state !== "deleting" || snapshot.deleteStep !== "confirm" || snapshot.liveMatch || deleteSecondsRemaining === null || deleteSecondsRemaining <= 0,
        reason: snapshot.liveMatch ? deleteReason : deleteSecondsRemaining === null ? accountCopy(locale, "delete.err.verify") : deleteSecondsRemaining <= 0 ? accountCopy(locale, "delete.err.window") : null,
      },
      keepAccount: { disabled: snapshot.state !== "deleting" },
    },
    friendRequestFailure: snapshot.requestFailed ? friendRequestFailureCopy(locale) : null,
  };
}

export function assertNever(value: never): never {
  throw new Error(`Unhandled account state: ${String(value)}`);
}

export type { AccountViewState };
