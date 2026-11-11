// Localized copy for the compact control window. Thai wording stays at the
// existing baseline; English provides the same messages and honest states.

export const OWNED_CONFIG_FILE = "gamestate_integration_kohbor.cfg";

export type Locale = "th" | "en";
export type Phase = "none" | "draft" | "pregame" | "live" | "post_game" | "unsupported";

const MESSAGES = {
  th: {
    languageLabel: "ภาษา",
    localeOptions: { th: "ไทย", en: "English" },
    overview: "ภาพรวม",
    loading: "กำลังเริ่มระบบ…",
    hostLost: "หน้าจอนี้ติดต่อระบบหลักของ Kohbor ไม่ได้",
    hostLostRestart: "ปิดแล้วเปิด Kohbor ใหม่",
    hostOutdated: "ค่าด้านล่างเป็นค่าล่าสุดที่ได้รับ ไม่ใช่ค่าปัจจุบัน",
    phases: {
      none: "รอเข้าเกม",
      draft: "เลือกฮีโร่",
      pregame: "ก่อนเริ่ม",
      live: "กำลังเล่น",
      post_game: "จบเกม",
      unsupported: "ไม่รองรับ",
    },
    settings: {
      unavailable: "การตั้งค่าภาษาไม่พร้อมใช้งาน",
      loadFailed: "อ่านการตั้งค่าภาษาไม่ได้",
      conflict: "การตั้งค่าเปลี่ยนจากที่อื่นแล้ว โหลดค่าล่าสุดให้แล้ว",
      refreshFailed: "ยืนยันการตั้งค่าภาษาล่าสุดไม่ได้",
      saveFailed: "บันทึกภาษาไม่ได้",
    },
    guide: {
      setupTitle: "เริ่มต้น: เชื่อมต่อ Kohbor กับ Dota 2",
      setupBody: (file: string) =>
        `เมื่อกดปุ่ม Kohbor จะสร้างไฟล์ ${file} หนึ่งไฟล์ในโฟลเดอร์ตั้งค่าของ Dota 2 เพื่อให้เกมส่งข้อมูลฮีโร่ของคุณมาที่เครื่องนี้ (127.0.0.1) ไม่แก้หรือลบไฟล์อื่น และข้อมูลไม่ออกนอกเครื่อง`,
      setupAction: "ตั้งค่าการเชื่อมต่อเกม",
      configConflictTitle: "ไฟล์ตั้งค่าของ Kohbor ถูกแก้จากภายนอก",
      configConflictBody: (file: string) =>
        `ไฟล์ ${file} ในโฟลเดอร์ Dota 2 ไม่ตรงกับที่ Kohbor เขียนไว้ จึงยังไม่เขียนทับ ถ้าต้องการให้ Kohbor ใช้งานได้ ให้แทนที่ไฟล์นี้`,
      replaceAction: "แทนที่ไฟล์ตั้งค่า…",
      tryAgain: "ลองอีกครั้ง",
      gameNotFoundTitle: "ไม่พบ Dota 2",
      gameNotFoundBody:
        "Kohbor มองหาเกมจากคลัง Steam ของบัญชี Windows นี้ และต้องพบเพียงที่เดียว ติดตั้งหรือเปิด Steam ให้เรียบร้อยแล้วลองอีกครั้ง — การเลือกโฟลเดอร์เกมเองยังไม่มีในบิลด์นี้",
      portUnavailableTitle: "พอร์ตรับข้อมูลถูกโปรแกรมอื่นใช้อยู่",
      portUnavailableBody:
        "ปิดโปรแกรมที่ใช้พอร์ตนั้นแล้วกดลองอีกครั้ง ถ้าพอร์ตยังไม่ว่าง Kohbor จะย้ายไปพอร์ตอื่นและเขียนไฟล์ตั้งค่าใหม่ ซึ่งต้องปิดแล้วเปิด Dota 2 ใหม่หนึ่งครั้ง",
      portsExhaustedTitle: "พอร์ต 3000–3010 ถูกใช้หมด",
      portsExhaustedBody: "ปิดโปรแกรมที่ใช้พอร์ตในช่วงนี้อย่างน้อยหนึ่งโปรแกรมแล้วลองอีกครั้ง ยังไม่มีการเขียนไฟล์ตั้งค่า",
      writeFailedTitle: "เขียนไฟล์ตั้งค่าไม่ได้",
      writeFailedBody: "ตรวจสิทธิ์การเขียนของโฟลเดอร์ Dota 2 และพื้นที่ว่างในดิสก์ แล้วลองอีกครั้ง ไฟล์เดิมยังอยู่ครบ",
      secretUnavailableTitle: "เข้าถึงที่เก็บความลับของ Windows ไม่ได้",
      secretUnavailableBody:
        "Kohbor เก็บโทเค็นของการเชื่อมต่อไว้กับบัญชี Windows ของคุณ ออกจากระบบ Windows แล้วเข้าใหม่ จากนั้นเปิด Kohbor อีกครั้ง ส่วนอื่นของแอปยังทำงานตามปกติ",
      intakeFailedTitle: "รับข้อมูลเกมไม่ได้",
      listeningTitle: "ปิดแล้วเปิด Dota 2 ใหม่หนึ่งครั้ง",
      listeningBody: (port: number | null) =>
        `Kohbor พร้อมรับข้อมูลที่พอร์ต ${port ?? "—"} แล้ว แต่ยังไม่มีข้อมูลเข้ามา เกมอ่านไฟล์ตั้งค่าตอนเปิดเท่านั้น ถ้าเกมเปิดค้างอยู่ก่อนตั้งค่าให้ปิดแล้วเปิดใหม่`,
      staleTitle: "ไม่ได้รับข้อมูลจากเกมแล้ว",
      staleBody: (age: string) => `ข้อมูลล่าสุดมาถึงเมื่อ ${age} ที่แล้ว ค่าที่แสดงอยู่เป็นค่าเก่า ถ้าปิดเกมไปแล้วไม่ต้องทำอะไร`,
      ownerUnverifiedTitle: "ยืนยันไม่ได้ว่าข้อมูลนี้เป็นของคุณ",
      ownerUnverifiedBody:
        "Kohbor แสดงค่าฮีโร่เฉพาะเมื่อบัญชี Steam ที่ล็อกอินตรงกับผู้เล่นในเกม ล็อกอิน Steam ให้เรียบร้อยแล้วปิดและเปิด Dota 2 ใหม่",
      replaceConfirm: (file: string) =>
        `ยืนยันการแทนที่: จะเขียนทับ ${file} และออกโทเค็นใหม่ สิ่งที่แก้ไว้ในไฟล์นี้จะหายไป และต้องปิดแล้วเปิด Dota 2 ใหม่หนึ่งครั้ง`,
      replaceAria: "ยืนยันการแทนที่ไฟล์ตั้งค่า",
      confirmReplace: "ยืนยันแทนที่",
      cancel: "ยกเลิก",
      busy: "กำลังดำเนินการ…",
    },
    readiness: {
      title: "ความพร้อม",
      note: "แต่ละส่วนแยกกัน — ส่วนหนึ่งพร้อมไม่ได้แปลว่าทั้งหมดพร้อม",
      intakeName: "รับข้อมูลเกม",
      intake: {
        not_configured: "ยังไม่ตั้งค่า",
        starting: "กำลังเริ่ม",
        listening: "รอข้อมูลจากเกม",
        receiving: "สด",
        stale: "ข้อมูลเก่า",
        unavailable: "ใช้ไม่ได้",
      },
      lastPortAge: (port: number | null, age: string) => `พอร์ต ${port ?? "—"} · ข้อมูลล่าสุด ${age} ที่แล้ว`,
      portWaiting: (port: number | null) => `พอร์ต ${port ?? "—"} · ยังไม่มีข้อมูลเข้ามา`,
      ownerName: "ยืนยันผู้เล่น",
      ownerUnknown: "ยังไม่ทราบ",
      ownerUnverified: "ยืนยันไม่ได้",
      ownerVerified: "ยืนยันแล้ว",
      ownerVerifiedOld: "ยืนยันแล้ว (ข้อมูลเก่า)",
      ownerUnknownDetail: "ตรวจกับบัญชี Steam ที่ล็อกอินเมื่อเข้าเกม",
      ownerVerifiedDetail: "บัญชี Steam ที่ล็อกอินตรงกับผู้เล่นในเกม",
      matchName: "แมตช์",
      staleSuffix: "ข้อมูลเก่า",
      session: (id: number) => `เซสชัน #${id}`,
      noMatch: "ยังไม่มีแมตช์",
      modeName: "โหมดเกม",
      modeUnknown: "ข้อมูลจากเกมไม่ระบุโหมด",
      criticalMediaName: "เสียงเตือนสำคัญ",
      mapVisibilityName: "การมองเห็นแผนที่",
      resourceName: "ทรัพยากรเครื่อง",
      resourceUnavailable: "วัดไม่ได้",
      ramOverBudget: "RAM เกินงบ",
      throttled: "ลดโหลด",
      normal: "ปกติ",
      resourceDetail: (cpu: number | null, ram: string) => `CPU ${cpu ?? "—"}% จากเพดาน 10% · RAM ${ram} MB จากงบ 900 MB`,
      accountName: "บัญชี",
      signedIn: "ล็อกอินแล้ว",
      signedOut: "ยังไม่ล็อกอิน",
      unused: "ไม่ได้ใช้",
      accountDetail: "ไม่บังคับ — ฟังก์ชันในเครื่องใช้ได้โดยไม่ต้องล็อกอิน",
      ready: "พร้อม",
      notReady: "ยังไม่พร้อม",
    },
    own: {
      title: "ฮีโร่ของคุณ",
      fresh: (age: string) => `สด · ${age}`,
      stale: (age: string) => `ข้อมูลเก่า · ${age} ที่แล้ว`,
      invalid: "ยืนยันผู้เล่นไม่ได้ — ไม่แสดงค่า",
      unsupported: "ไม่รองรับ — ผู้ชม รีเพลย์ หรือเกมคัสตอม",
      noData: "ยังไม่มีข้อมูล",
      waitingGame: "รอเริ่มเกม — ค่าจะแสดงเมื่อเข้าเกม",
      gameEnded: "จบเกมแล้ว",
      waitingStart: "ยังไม่มีข้อมูล — รอเข้าเกม",
      invalidValue: "ค่าจากเกมไม่ถูกต้อง",
      invalidValueShort: "ไม่ถูกต้อง",
      hero: "ฮีโร่",
      level: "เลเวล",
      mana: "มานา",
      state: "สถานะ",
      team: "ทีม",
      alive: "มีชีวิต",
      dead: "ตาย",
      respawnPrefix: "เกิดใหม่ใน",
      secondsUnit: "วิ",
    },
  },
  en: {
    languageLabel: "Language",
    localeOptions: { th: "ไทย", en: "English" },
    overview: "Overview",
    loading: "Starting…",
    hostLost: "This window cannot reach the Kohbor host",
    hostLostRestart: "Close and reopen Kohbor",
    hostOutdated: "Values below are the latest received, not current",
    phases: {
      none: "Waiting for game",
      draft: "Hero selection",
      pregame: "Pre-game",
      live: "In game",
      post_game: "Game ended",
      unsupported: "Unsupported",
    },
    settings: {
      unavailable: "Language settings unavailable",
      loadFailed: "Could not read language settings",
      conflict: "Settings changed elsewhere. Latest settings loaded.",
      refreshFailed: "Could not confirm the latest language settings",
      saveFailed: "Could not save language",
    },
    guide: {
      setupTitle: "Get started: connect Kohbor to Dota 2",
      setupBody: (file: string) =>
        `When you select the button, Kohbor creates one ${file} file in the Dota 2 settings folder so the game can send your hero data to this computer (127.0.0.1). It does not edit or remove other files, and data stays on this computer.`,
      setupAction: "Set up game connection",
      configConflictTitle: "Kohbor's settings file was changed externally",
      configConflictBody: (file: string) =>
        `${file} in the Dota 2 folder no longer matches the file Kohbor wrote, so it has not been overwritten. Replace this file if you want Kohbor to use the connection.`,
      replaceAction: "Replace settings file…",
      tryAgain: "Try again",
      gameNotFoundTitle: "Dota 2 was not found",
      gameNotFoundBody:
        "Kohbor looks for the game in this Windows account's Steam library and requires exactly one match. Install or open Steam, then try again. Selecting the game folder manually is not available in this build.",
      portUnavailableTitle: "Another program is using the intake port",
      portUnavailableBody:
        "Close the program using that port and try again. If it remains unavailable, Kohbor will move to another port and rewrite its settings file; restart Dota 2 once afterward.",
      portsExhaustedTitle: "Ports 3000–3010 are all in use",
      portsExhaustedBody: "Close at least one program using a port in this range, then try again. No settings file has been written.",
      writeFailedTitle: "Could not write the settings file",
      writeFailedBody: "Check write access to the Dota 2 folder and available disk space, then try again. The previous file is intact.",
      secretUnavailableTitle: "Could not access Windows secret storage",
      secretUnavailableBody:
        "Kohbor stores the connection token with your Windows account. Sign out of Windows and back in, then restart Kohbor. Other app functions continue to work.",
      intakeFailedTitle: "Could not receive game data",
      listeningTitle: "Restart Dota 2 once",
      listeningBody: (port: number | null) =>
        `Kohbor is ready to receive data on port ${port ?? "—"}, but none has arrived yet. The game reads its settings file only at startup. If the game was already open, restart it.`,
      staleTitle: "Game data stopped arriving",
      staleBody: (age: string) => `The latest data arrived ${age} ago. Displayed values are old. No action is needed if you closed the game.`,
      ownerUnverifiedTitle: "Could not verify that this data belongs to you",
      ownerUnverifiedBody:
        "Kohbor shows hero values only when the signed-in Steam account matches the player in game. Sign in to Steam, then restart Dota 2.",
      replaceConfirm: (file: string) =>
        `Confirm replacement: ${file} will be overwritten and a new token issued. Changes in this file will be lost, and Dota 2 must be restarted once.`,
      replaceAria: "Confirm settings file replacement",
      confirmReplace: "Confirm replacement",
      cancel: "Cancel",
      busy: "Working…",
    },
    readiness: {
      title: "Readiness",
      note: "Each capability is independent; one being ready does not mean all are ready",
      intakeName: "Game data",
      intake: {
        not_configured: "Not configured",
        starting: "Starting",
        listening: "Waiting for game data",
        receiving: "Live",
        stale: "Stale data",
        unavailable: "Unavailable",
      },
      lastPortAge: (port: number | null, age: string) => `Port ${port ?? "—"} · last data ${age} ago`,
      portWaiting: (port: number | null) => `Port ${port ?? "—"} · no data received yet`,
      ownerName: "Player identity",
      ownerUnknown: "Unknown",
      ownerUnverified: "Unverified",
      ownerVerified: "Verified",
      ownerVerifiedOld: "Verified (stale data)",
      ownerUnknownDetail: "Checked against the Steam account signed in when entering the game",
      ownerVerifiedDetail: "Signed-in Steam account matches the in-game player",
      matchName: "Match",
      staleSuffix: "stale data",
      session: (id: number) => `Session #${id}`,
      noMatch: "No match yet",
      modeName: "Game mode",
      modeUnknown: "Game data does not identify the mode",
      criticalMediaName: "Critical audio",
      mapVisibilityName: "Map visibility",
      resourceName: "System resources",
      resourceUnavailable: "Unavailable",
      ramOverBudget: "RAM over budget",
      throttled: "Throttled",
      normal: "Normal",
      resourceDetail: (cpu: number | null, ram: string) => `CPU ${cpu ?? "—"}% of 10% cap · RAM ${ram} MB of 900 MB budget`,
      accountName: "Account",
      signedIn: "Signed in",
      signedOut: "Signed out",
      unused: "Not used",
      accountDetail: "Optional — local functions work without signing in",
      ready: "Ready",
      notReady: "Not ready",
    },
    own: {
      title: "Your hero",
      fresh: (age: string) => `Live · ${age}`,
      stale: (age: string) => `Stale · ${age} ago`,
      invalid: "Player could not be verified — values hidden",
      unsupported: "Unsupported — spectator, replay, or custom game",
      noData: "No data yet",
      waitingGame: "Waiting for game — values appear after entering a match",
      gameEnded: "Game ended",
      waitingStart: "No data yet — waiting for game",
      invalidValue: "Invalid value from game",
      invalidValueShort: "invalid",
      hero: "Hero",
      level: "Level",
      mana: "Mana",
      state: "State",
      team: "Team",
      alive: "Alive",
      dead: "Dead",
      respawnPrefix: "Respawn in",
      secondsUnit: "s",
    },
  },
} as const;

export function copy(locale: Locale) {
  return MESSAGES[locale];
}

const REASON_TH: Record<string, string> = {
  SOURCE_UNAVAILABLE: "ยังไม่ได้ตั้งค่าการเชื่อมต่อเกม",
  SOURCE_STALE: "ข้อมูลจากเกมเก่าเกิน 2 วินาที",
  SOURCE_INVALID: "ข้อมูลที่ได้รับไม่ใช่ข้อมูลจาก Dota 2",
  OWNER_UNVERIFIED: "ยืนยันผู้เล่นไม่ได้ — ล็อกอิน Steam แล้วเปิด Dota 2 ใหม่",
  METRIC_UNAVAILABLE: "วัดค่าไม่ได้ในขณะนี้ — การจำกัดทรัพยากรยังทำงานอยู่",
  GSI_GAME_NOT_FOUND: "ไม่พบ Dota 2 ในคลัง Steam ของเครื่องนี้ หรือพบมากกว่าหนึ่งที่",
  GSI_CONFIG_CONFLICT: "ไฟล์ตั้งค่าถูกแก้จากภายนอก — ยังไม่เขียนทับ",
  GSI_CONFIG_WRITE_FAILED: "เขียนไฟล์ตั้งค่าในโฟลเดอร์ Dota 2 ไม่ได้",
  GSI_PORT_UNAVAILABLE: "พอร์ตที่ตั้งไว้ถูกโปรแกรมอื่นใช้อยู่",
  GSI_PORT_EXHAUSTED: "พอร์ต 3000–3010 ถูกใช้หมด",
  GSI_SECRET_UNAVAILABLE: "เข้าถึงที่เก็บความลับของ Windows ไม่ได้",
  MEDIA_UNMAPPED: "แพ็กที่เลือกยังไม่มีเสียงเตือนสำคัญที่ผ่านการตรวจ",
  CAPABILITY_UNAVAILABLE: "ยังไม่มีในบิลด์นี้",
  VERSION_UNSUPPORTED: "เวอร์ชันหรือการตั้งค่าที่ได้รับใช้ร่วมกันไม่ได้",
};

const REASON_EN: Record<string, string> = {
  SOURCE_UNAVAILABLE: "Game connection is not configured",
  SOURCE_STALE: "Game data is more than 2 seconds old",
  SOURCE_INVALID: "Received data is not valid Dota 2 data",
  OWNER_UNVERIFIED: "Player identity is unverified — sign in to Steam and restart Dota 2",
  METRIC_UNAVAILABLE: "Metric is unavailable — resource containment remains active",
  GSI_GAME_NOT_FOUND: "Dota 2 was not found in this computer's Steam library, or multiple copies were found",
  GSI_CONFIG_CONFLICT: "Settings file was changed externally — it has not been overwritten",
  GSI_CONFIG_WRITE_FAILED: "Could not write the settings file in the Dota 2 folder",
  GSI_PORT_UNAVAILABLE: "Another program is using the configured port",
  GSI_PORT_EXHAUSTED: "All ports 3000–3010 are in use",
  GSI_SECRET_UNAVAILABLE: "Could not access Windows secret storage",
  MEDIA_UNMAPPED: "The selected package has no verified critical alert audio",
  CAPABILITY_UNAVAILABLE: "Not available in this build",
  VERSION_UNSUPPORTED: "Settings version or revision is not compatible",
};

/** Localized reason. An unmapped code is shown with a plain-language lead, never alone. */
export function reason(code: string | null | undefined, locale: Locale = "th"): string {
  if (!code) return "";
  const messages = locale === "th" ? REASON_TH : REASON_EN;
  return messages[code] ?? (locale === "th" ? `เกิดข้อผิดพลาดที่ยังไม่มีคำอธิบาย (${code})` : `Unexplained error (${code})`);
}

export function seconds(ms: number | null | undefined, locale: Locale = "th"): string {
  if (ms === null || ms === undefined) return "—";
  const value = (ms / 1000).toFixed(1);
  return locale === "th" ? `${value} วิ` : `${value} s`;
}
