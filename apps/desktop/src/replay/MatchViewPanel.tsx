import { useEffect, useRef, useState, type ChangeEvent } from "react";
import type { Locale } from "../copy";
import { BUNDLED } from "./bundledIcons";
import {
  admitMatchBundle, countFound, gameModeName, iconResolver, MAX_BUNDLE_BYTES, neededIconKeys,
  type BundleError, type MatchBundle, type PickedFile,
} from "./matchBundle";
import { calibrationMatches } from "./matchTables.mjs";
import { mountMatchView } from "./matchViewer.mjs";
import markup from "./matchViewer.html?raw";
import style from "./matchViewer.css?raw";

const MiB = (bytes: number) => Math.round(bytes / (1024 * 1024));

const COPY = {
  th: {
    bundle: "เปิด Match Bundle: เลือก manifest.json, match.json และ world.json พร้อมกัน",
    reading: "กำลังอ่านไฟล์…",
    iconCount: (found: number, needed: number, build: string | null) =>
      `ไอคอนที่พบ ${found} จาก ${needed}${build ? ` (ชุดที่มากับ Kohbor จากเกมบิลด์ ${build})` : ""} ที่เหลือแสดงเป็นกรอบชื่อ`,
    memory: "ไฟล์ทั้งหมดอยู่ในหน่วยความจำเท่านั้น ไม่บันทึกและไม่ส่งออกนอกเครื่อง ปิดแท็บนี้แล้วข้อมูลจะหายไป",
    proto: "ต้นแบบ: ใช้ตารางของแมตช์ตัวอย่างแมตช์เดียว (การแจ้งเตือน ult และ CC, ปุ่มคู่ Mid, พิกัดมินิแมป) แมตช์อื่นอาจแจ้งเตือนไม่ครบหรือพิกัดคลาดเคลื่อน",
    calibration: "พิกัดมินิแมปของ bundle นี้ต่างจากตารางต้นแบบ ตำแหน่งบนแผนที่อาจคลาดเคลื่อน",
    lede: (patch: string, mode: string, length: string) => `แพตช์ ${patch} · ${mode} · ยาว ${length} · อ่านจากไฟล์ในเครื่องนี้`,
    unknownPatch: "ไม่ทราบ",
    unknownMode: "ไม่ทราบโหมดเกม",
    unknownLength: "ไม่ทราบ",
    error: (e: BundleError): string => {
      switch (e.code) {
        case "missing": return `ไม่ครบ ขาด ${e.files.join(", ")}`;
        case "duplicate": return `เลือก ${e.file} มากกว่าหนึ่งไฟล์`;
        case "too-large": return `${e.file} ใหญ่เกิน ${MiB(e.limit)} MB`;
        case "unreadable": return `อ่าน ${e.file} ไม่ได้ (ไม่ใช่ JSON แบบ UTF-8)`;
        case "schema": return `${e.file} เป็น schema ${e.found} แต่รองรับเฉพาะ 3.0`;
        case "integrity": return `${e.file} ไม่ตรงกับขนาดหรือ SHA-256 ใน manifest.json`;
        case "shape": return `${e.file} มีโครงสร้างไม่ถูกต้องที่ ${e.path}`;
      }
    },
  },
  en: {
    bundle: "Open a Match Bundle: select manifest.json, match.json and world.json together",
    reading: "Reading files…",
    iconCount: (found: number, needed: number, build: string | null) =>
      `Found ${found} of ${needed} icons${build ? ` (bundled with Kohbor from game build ${build})` : ""}; the rest show as name tiles.`,
    memory: "Files stay in memory only. Nothing is saved or sent anywhere. Leaving this tab discards them.",
    proto: "Prototype: one-match tables drive ult and CC alerts, the Mid pair button and the minimap projection. Other matches can miss alerts or be offset on the map. Labels written by the prototype script are still Thai.",
    calibration: "This bundle's minimap calibration differs from the prototype table; map positions can be offset.",
    lede: (patch: string, mode: string, length: string) => `Patch ${patch} · ${mode} · ${length} · read from files on this computer`,
    unknownPatch: "unknown",
    unknownMode: "game mode unknown",
    unknownLength: "length unknown",
    error: (e: BundleError): string => {
      switch (e.code) {
        case "missing": return `Incomplete bundle: missing ${e.files.join(", ")}.`;
        case "duplicate": return `More than one ${e.file} was selected.`;
        case "too-large": return `${e.file} is larger than ${MiB(e.limit)} MB.`;
        case "unreadable": return `${e.file} could not be read as UTF-8 JSON.`;
        case "schema": return `${e.file} uses schema ${e.found}; only 3.0 is supported.`;
        case "integrity": return `${e.file} does not match the size or SHA-256 recorded in manifest.json.`;
        case "shape": return `${e.file} has an unexpected structure at ${e.path}.`;
      }
    },
  },
};

const clock = (s: number) => `${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, "0")}`;

interface Loaded {
  bundle: MatchBundle;
  found: number;
  needed: number;
}

export function MatchViewPanel({ locale }: { locale: Locale }) {
  const text = COPY[locale];
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [error, setError] = useState<BundleError | null>(null);
  const [reading, setReading] = useState(false);
  const bundleFiles = useRef<PickedFile[]>([]);
  const target = useRef<HTMLDivElement>(null);
  const request = useRef(0);

  useEffect(() => () => { request.current++; }, []);
  useEffect(() => {
    const frame = target.current;
    if (!loaded || !frame) return;
    const { bundle } = loaded;
    const icons = BUNDLED.icons;
    const root = frame.shadowRoot ?? frame.attachShadow({ mode: "open" });
    const mode = gameModeName(bundle.gameMode) ?? text.unknownMode;
    const patch = bundle.patch ?? text.unknownPatch;
    const notes = [text.proto];
    if (!calibrationMatches(bundle.calibration)) notes.push(text.calibration);
    return mountMatchView(root, bundle, {
      locale, markup, style,
      icon: iconResolver(icons),
      hasMap: icons.has("map:"),
      header: {
        "mv-lede": text.lede(patch, mode, bundle.durationS === null ? text.unknownLength : clock(bundle.durationS)),
        "mv-sub": `${patch} · ${mode}`,
        "mv-proto": notes.join(" "),
      },
    });
  }, [loaded, locale, text]);

  async function load() {
    const ticket = ++request.current;
    setReading(true);
    setError(null);
    const admission = await admitMatchBundle(bundleFiles.current);
    if (ticket !== request.current) return;
    if (!admission.ok) {
      setReading(false);
      setLoaded(null);
      setError(admission.error);
      return;
    }
    const bundle = admission.bundle;
    const keys = neededIconKeys(bundle);
    setReading(false);
    setLoaded({ bundle, found: countFound(BUNDLED.icons, keys), needed: keys.size });
  }

  const pick = (event: ChangeEvent<HTMLInputElement>) => {
    bundleFiles.current = Array.from(event.currentTarget.files ?? []);
    event.currentTarget.value = "";
    if (bundleFiles.current.length) void load();
  };

  return (
    <>
      <section className="card">
        <label>
          {text.bundle}
          <input type="file" multiple accept=".json,application/json" onChange={pick} />
        </label>
        <p>{text.memory} {locale === "th" ? "ขนาดสูงสุด" : "Size limits"}: match.json {MiB(MAX_BUNDLE_BYTES["match.json"])} MB, world.json {MiB(MAX_BUNDLE_BYTES["world.json"])} MB.</p>
        {reading && <p role="status">{text.reading}</p>}
        {error && <p role="alert">{text.error(error)}</p>}
        {loaded && !reading && (
          <p role="status">{text.iconCount(loaded.found, loaded.needed, BUNDLED.build)}</p>
        )}
      </section>
      <div ref={target} />
    </>
  );
}
