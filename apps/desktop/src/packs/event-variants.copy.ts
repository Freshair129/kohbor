export interface EventVariantsCopy {
  title: string;
  selectionLabel: string;
  selection: Record<"random" | "shuffle" | "sequence", string>;
  voice: (number: number) => string;
  variantId: string;
  clipId: string;
  caption: string;
  captionMissing: string;
  duration: string;
  secondsUnit: string;
  bannerYes: string;
  bannerNo: string;
  play: string;
  playing: string;
  noVariants: string;
  audioNotStarted: string;
  audioStarted: string;
  audioEnded: string;
  visualShown: string;
  visualFailed: string;
  visualNone: string;
  visualUnknown: string;
  auditionFailed: string;
  auditionFailure: Record<string, string>;
}

const copy: Record<"th" | "en", EventVariantsCopy> = {
  th: {
    title: "ตัวเลือกเสียง",
    selectionLabel: "การเลือก",
    selection: {
      random: "สุ่ม",
      shuffle: "สุ่มจนครบทุกเสียง",
      sequence: "วนตามลำดับ",
    },
    voice: (number: number) => `เสียงที่ ${number}`,
    variantId: "ตัวเลือก",
    clipId: "รหัสคลิป",
    caption: "คำพูด",
    captionMissing: "ยังไม่มีคำบรรยาย",
    duration: "ความยาว",
    secondsUnit: "วินาที",
    bannerYes: "มีแบนเนอร์",
    bannerNo: "ไม่มีแบนเนอร์",
    play: "เล่นตัวอย่าง",
    playing: "กำลังเล่น",
    noVariants: "ไม่มีตัวเลือกเสียง",
    audioNotStarted: "ยังไม่เริ่มเสียง",
    audioStarted: "เริ่มเสียงแล้ว",
    audioEnded: "เสียงจบแล้ว",
    visualShown: "แสดงแบนเนอร์แล้ว",
    visualFailed: "แสดงแบนเนอร์ไม่สำเร็จ",
    visualNone: "ไม่มีภาพแบนเนอร์",
    visualUnknown: "ยังยืนยันผลภาพไม่ได้",
    auditionFailed: "ทดลองเล่นไม่สำเร็จ",
    auditionFailure: {
      AUDITION_MATCH_ACTIVE: "ทดลองฟังได้เมื่อออกจากแมตช์แล้ว",
      AUDITION_NO_ACTIVE_PACK: "เปิดใช้แพ็กก่อนทดลองฟัง",
      AUDITION_UNMAPPED: "ไม่พบเสียงของเหตุการณ์นี้ในแพ็ก",
      AUDITION_VARIANT_UNAVAILABLE: "ตัวเลือกเสียงนี้ไม่พร้อมเล่น",
      AUDITION_MEDIA_STATE_UNKNOWN: "ตรวจสอบเสียงในแพ็กที่เปิดใช้อยู่ไม่ได้",
      AUDITION_BUSY: "ช่องเสียงกำลังใช้งานอยู่",
      AUDITION_MUTED: "เปิดเสียงก่อนทดลองฟัง",
      AUDITION_EVENT_UNAVAILABLE: "เหตุการณ์นี้ยังไม่มีเสียงที่ทดลองได้",
    },
  },
  en: {
    title: "Voice variants",
    selectionLabel: "Selection",
    selection: {
      random: "Random",
      shuffle: "Shuffle without repeats",
      sequence: "In order",
    },
    voice: (number: number) => `Voice ${number}`,
    variantId: "Variant",
    clipId: "Clip ID",
    caption: "Spoken words",
    captionMissing: "No caption supplied",
    duration: "Duration",
    secondsUnit: "s",
    bannerYes: "Banner yes",
    bannerNo: "Banner no",
    play: "Play sample",
    playing: "Playing",
    noVariants: "No voice variants",
    audioNotStarted: "Audio did not start",
    audioStarted: "Audio started",
    audioEnded: "Audio ended",
    visualShown: "Banner shown",
    visualFailed: "Banner failed",
    visualNone: "No banner for this clip",
    visualUnknown: "Banner outcome unknown",
    auditionFailed: "Audition failed",
    auditionFailure: {
      AUDITION_MATCH_ACTIVE: "Audition is available after leaving the match",
      AUDITION_NO_ACTIVE_PACK: "Activate a pack before auditioning a clip",
      AUDITION_UNMAPPED: "This event has no mapped clip in the pack",
      AUDITION_VARIANT_UNAVAILABLE: "This voice variant is unavailable",
      AUDITION_MEDIA_STATE_UNKNOWN: "Could not verify the active pack's clip",
      AUDITION_BUSY: "The audio channel is busy",
      AUDITION_MUTED: "Turn audio on before auditioning",
      AUDITION_EVENT_UNAVAILABLE: "This event has no auditionable clip",
    },
  },
};

export function eventVariantsCopy(locale: "th" | "en"): EventVariantsCopy {
  return copy[locale];
}
