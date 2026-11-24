import { useEffect, useState, type ReactNode } from "react";
import { copy, type Locale } from "../copy";
import {
  DEFAULT_SETTINGS_SECTION,
  initialSettingsSection,
  readSettingsSection,
  rememberSettingsSection,
  SETTINGS_SECTIONS,
  type SettingsSectionId,
} from "./settingsModel";

export interface SettingsPageProps {
  locale: Locale;
  sections: Partial<Record<SettingsSectionId, ReactNode>>;
  objectiveAlerts?: ReactNode;
  /** Set to `overlay-layout` when Settings is opened from a layout action. */
  initialSection?: "overlay-layout" | null;
}

const rendererStorage = {
  getItem: (key: string) => window.localStorage.getItem(key),
  setItem: (key: string, value: string) => window.localStorage.setItem(key, value),
};

export function SettingsPage({ locale, sections, objectiveAlerts, initialSection = null }: SettingsPageProps) {
  const text = copy(locale);
  const [activeSection, setActiveSection] = useState<SettingsSectionId>(() => {
    const preferredEntry = initialSection === "overlay-layout" ? "layout" : null;
    return initialSettingsSection(preferredEntry, readSettingsSection(rendererStorage));
  });

  useEffect(() => {
    rememberSettingsSection(activeSection, rendererStorage);
  }, [activeSection]);

  useEffect(() => {
    if (initialSection === "overlay-layout") setActiveSection(DEFAULT_SETTINGS_SECTION);
  }, [initialSection]);

  return (
    <div className="settings-layout">
      <nav className="settings-nav" aria-label={text.shell.settingsNavigationLabel}>
        <h2 className="settings-nav-title">{text.shell.destinations.settings}</h2>
        {SETTINGS_SECTIONS.map((section) => {
          const label = text.shell.settingsSections[section.id];
          return (
            <button key={section.id} type="button" aria-current={activeSection === section.id ? "page" : undefined}
              aria-controls={`settings-section-${section.id}`} onClick={() => setActiveSection(section.id)}>
              {label}
            </button>
          );
        })}
      </nav>
      <div className="settings-content" role="region" aria-label={text.shell.settingsContentLabel} tabIndex={0}>
        {SETTINGS_SECTIONS.map((section) => {
          const label = text.shell.settingsSections[section.id];
          const content = section.id === "alerts-objectives" ? objectiveAlerts : sections[section.id];
          return (
            <section key={section.id} id={`settings-section-${section.id}`} className="settings-section"
              aria-labelledby={`settings-section-title-${section.id}`} hidden={activeSection !== section.id}>
              <header className="settings-section-heading">
                <h2 id={`settings-section-title-${section.id}`}>{label}</h2>
              </header>
              {content}
            </section>
          );
        })}
      </div>
    </div>
  );
}
