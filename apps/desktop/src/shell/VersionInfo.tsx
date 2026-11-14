import type { Locale } from "../copy";
import type { AppVersionView } from "./appVersionModel";

export function VersionHeader({ view }: { view: AppVersionView }) {
  return <div className="shell-version" aria-label="รุ่นของแอป / App version">
    <span className="shell-version-number mono" title={view.compact}>{view.compact}</span>
    {view.demoBadge && <span className="shell-version-demo">{view.demoBadge}</span>}
  </div>;
}

export function AboutPanel({ view, locale }: { view: AppVersionView; locale: Locale }) {
  const labels = locale === "th" ? ["รุ่น", "Commit", "วันที่ build", "โปรไฟล์"] : ["Version", "Commit", "Build date", "Profile"];
  const values = [view.version, view.commit, view.buildDate, view.profile];
  return <section className="shell-about" aria-labelledby="shell-about-heading">
    <h2 id="shell-about-heading">เกี่ยวกับ / About</h2>
    {view.demoBadge && <span className="shell-version-demo">{view.demoBadge}</span>}
    <dl>{labels.map((label, index) => <div key={label}><dt>{label}</dt><dd title={values[index]}>{values[index]}</dd></div>)}</dl>
  </section>;
}
