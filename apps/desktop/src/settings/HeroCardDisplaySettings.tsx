import { useEffect, useState } from "react";
import { STAT_ICON_CATALOG } from "../live/statIcons/catalog";
import { statIconUrl } from "../live/statIcons/assets";
import {
  CORE_HERO_CARD_DISPLAY,
  DEFAULT_HERO_CARD_DISPLAY,
  HERO_CARD_DISPLAY_EVENT,
  HERO_CARD_DISPLAY_KEY,
  HERO_CARD_GROUPS,
  HERO_CARD_GROUP_LABELS,
  reflowHeroCardDisplay,
  readHeroCardDisplay,
  setHeroCardDisplayGroup,
  writeHeroCardDisplay,
  type HeroCardDisplay,
  type HeroCardGroup,
} from "../shared/heroCardDisplay.mjs";
import type { Locale } from "../copy";
import "./heroCardDisplaySettings.css";

const groupHelp: Record<HeroCardGroup, [string, string]> = {
  attrs: ["แถวคุณสมบัติ", "Attributes row"], dmg: ["แถวคุณสมบัติ", "Attributes row"],
  reg: ["บนหลอด HP / มานาใน Replay", "On Replay HP / mana bars"],
  arm: ["สถิติรอง", "Derived stats"], mr: ["สถิติรอง", "Derived stats"], amp: ["สถิติรอง", "Derived stats"],
  as: ["สถิติรอง", "Derived stats"], ms: ["สถิติรอง", "Derived stats"], ev: ["สถิติรอง", "Derived stats"],
  sr: ["สถิติรอง", "Derived stats"], ls: ["สถิติรอง", "Derived stats"],
  rates: ["เฉพาะหน้า Live", "Live only"], status: ["เฉพาะ Replay ในตอนนี้", "Replay only today"],
};

const settingsGroups: { title: [string, string]; groups: HeroCardGroup[] }[] = [
  { title: ["หลัก", "Core"], groups: ["attrs", "dmg", "reg"] },
  { title: ["ป้องกันและเวท", "Defence and spell"], groups: ["arm", "mr", "amp", "sr"] },
  { title: ["โจมตีและเคลื่อนที่", "Attack and movement"], groups: ["as", "ms", "ev", "ls"] },
  { title: ["อื่น ๆ", "Other"], groups: ["rates", "status"] },
];

const groupStatKeys: Partial<Record<HeroCardGroup, (keyof typeof STAT_ICON_CATALOG)[]>> = {
  attrs: ["str", "agi", "int"], dmg: ["dmg"], reg: ["reg"], arm: ["arm"], mr: ["mr"], amp: ["amp"],
  as: ["as"], ms: ["ms"], ev: ["ev"], sr: ["sr"], ls: ["ls"], rates: ["gpm", "xpm"],
};

export function HeroCardDisplayControls({ value, locale, onChange, compact = false }: {
  value: HeroCardDisplay;
  locale: Locale;
  onChange: (value: HeroCardDisplay) => void;
  compact?: boolean;
}) {
  const thai = locale !== "en";
  const reflow = reflowHeroCardDisplay(value, "full");
  const previewLabels: Partial<Record<HeroCardGroup, [string, string]>> = {
    attrs: [HERO_CARD_GROUP_LABELS.attrs, "Attributes"], dmg: [HERO_CARD_GROUP_LABELS.dmg, "Damage"],
    reg: [HERO_CARD_GROUP_LABELS.reg, "Regeneration"], arm: [HERO_CARD_GROUP_LABELS.arm, "Armour"],
    mr: [HERO_CARD_GROUP_LABELS.mr, "Magic resistance"], amp: [HERO_CARD_GROUP_LABELS.amp, "Spell amplification"],
    as: [HERO_CARD_GROUP_LABELS.as, "Attack speed"], ms: [HERO_CARD_GROUP_LABELS.ms, "Move speed"],
    ev: [HERO_CARD_GROUP_LABELS.ev, "Evasion"], sr: [HERO_CARD_GROUP_LABELS.sr, "Status resistance"],
    ls: [HERO_CARD_GROUP_LABELS.ls, "Lifesteal"], rates: [HERO_CARD_GROUP_LABELS.rates, "GPM / XPM · Live only"],
  };
  const previewAttributes = (["attrs", "dmg"] as HeroCardGroup[]).filter(group => value.show[group]);
  const previewDerived = (["arm", "mr", "amp", "as", "ms", "ev", "sr", "ls"] as HeroCardGroup[]).filter(group => value.show[group]);
  const setGroup = (group: HeroCardGroup, checked: boolean) => onChange(setHeroCardDisplayGroup(value, group, checked));
  return <div className={`hero-card-display-controls${compact ? " is-compact" : ""}`}>
    <div className="hero-card-display-actions">
      <button type="button" onClick={() => onChange(DEFAULT_HERO_CARD_DISPLAY)}>{thai ? "แสดงทั้งหมด" : "Show all"}</button>
      <button type="button" onClick={() => onChange(CORE_HERO_CARD_DISPLAY)}>{thai ? "เฉพาะค่าหลัก" : "Core stats"}</button>
      <button type="button" onClick={() => onChange(DEFAULT_HERO_CARD_DISPLAY)}>{thai ? "คืนค่าเริ่มต้น" : "Reset defaults"}</button>
    </div>
    <div className="hero-card-display-grid">
      {settingsGroups.map(section => <section className="hero-card-display-group" key={section.title[1]}>
        <h3>{section.title[thai ? 0 : 1]}</h3>
        {section.groups.map(group => <label key={group}>
          <input type="checkbox" checked={value.show[group]} onChange={event => setGroup(group, event.currentTarget.checked)} />
          <span className="hero-card-display-icons" aria-hidden="true">
            {(groupStatKeys[group] ?? []).flatMap(key => STAT_ICON_CATALOG[key].iconIds).map((iconId, index) => {
              const src = statIconUrl(iconId);
              return src ? <img key={`${iconId}-${index}`} src={src} alt="" /> : null;
            })}
            {group === "status" && <b>ⓘ</b>}
          </span>
          <span>{thai ? HERO_CARD_GROUP_LABELS[group] : group === "attrs" ? "STR / AGI / INT"
            : group === "dmg" ? "Attack damage" : group === "reg" ? "HP / mana regeneration"
              : group === "arm" ? "Armour" : group === "mr" ? "Magic resistance"
                : group === "amp" ? "Spell amplification" : group === "as" ? "Attack speed"
                  : group === "ms" ? "Move speed" : group === "ev" ? "Evasion"
                    : group === "sr" ? "Status resistance" : group === "ls" ? "Lifesteal"
                      : group === "rates" ? "GPM / XPM" : "Buffs and debuffs"}</span>
          <small>{groupHelp[group][thai ? 0 : 1]}</small>
        </label>)}
      </section>)}
    </div>
    <div className="hero-card-display-preview" aria-label={thai ? "ตัวอย่างการเรียงการ์ด" : "Card layout example"}>
      <div className="hero-card-display-preview-head"><b>{thai ? "ฮีโร่" : "Hero"}</b><span>{thai ? "เลเวล · K/D/A" : "Level · K/D/A"}</span></div>
      <div className="hero-card-display-preview-bars"><i aria-hidden="true" /><i aria-hidden="true" />
        {value.show.reg && <small>{thai ? "ฟื้นฟู HP / มานา" : "HP / mana regeneration"}</small>}
      </div>
      <div className="hero-card-display-preview-fixed"><span>{thai ? "สกิล" : "Abilities"}</span><span>{thai ? "ไอเทม" : "Items"}</span></div>
      {previewAttributes.length > 0 && <div className="hero-card-display-preview-attributes">
        {previewAttributes.map(group => <span key={group}>{previewLabels[group]?.[thai ? 0 : 1] ?? HERO_CARD_GROUP_LABELS[group]}</span>)}
      </div>}
      {previewDerived.length > 0 && <div className="hero-card-display-preview-fields" data-preview-columns={reflow.derivedColumns}>
        {previewDerived.map(group => <span key={group}>{previewLabels[group]?.[thai ? 0 : 1] ?? HERO_CARD_GROUP_LABELS[group]}</span>)}
      </div>}
      {value.show.rates && <span className="hero-card-display-preview-rates">{previewLabels.rates?.[thai ? 0 : 1]} · {thai ? "Live เท่านั้น" : "Live only"}</span>}
      {reflow.showStatus && <span className="hero-card-display-preview-status">◷ {thai ? "บัฟและดีบัฟ" : "Buffs and debuffs"}</span>}
      <small>{thai ? "ซ่อนช่องแล้วแถวจะจัดใหม่โดยไม่เว้นช่องว่าง" : "Hidden cells reflow without empty gaps"}</small>
    </div>
  </div>;
}

export function HeroCardDisplaySettings({ locale }: { locale: Locale }) {
  const storage = (() => { try { return window.localStorage; } catch { return undefined; } })();
  const [value, setValue] = useState(() => readHeroCardDisplay(storage));
  useEffect(() => {
    const sync = (event: StorageEvent) => {
      if (event.key === HERO_CARD_DISPLAY_KEY) setValue(readHeroCardDisplay(storage));
    };
    const syncSameWindow = (event: Event) => {
      const next = (event as CustomEvent<{ value?: HeroCardDisplay }>).detail?.value;
      if (next) setValue(next);
    };
    window.addEventListener("storage", sync);
    window.addEventListener(HERO_CARD_DISPLAY_EVENT, syncSameWindow);
    return () => {
      window.removeEventListener("storage", sync);
      window.removeEventListener(HERO_CARD_DISPLAY_EVENT, syncSameWindow);
    };
  }, [storage]);
  const change = (next: HeroCardDisplay) => setValue(writeHeroCardDisplay(storage, next));
  return <section className="hero-card-display-settings" aria-label={locale === "en" ? "Hero card display" : "การ์ดฮีโร่"}>
    <p>{locale === "en"
      ? "Choose the same card fields for Live and Replay/Analytics. All fields start on."
      : "เลือกข้อมูลที่แสดงในการ์ดฮีโร่ ใช้ค่าเดียวกันกับ Live และ Replay/Analytics โดยเปิดทุกช่องไว้เป็นค่าเริ่มต้น"}</p>
    <HeroCardDisplayControls value={value} locale={locale} onChange={change} />
  </section>;
}
