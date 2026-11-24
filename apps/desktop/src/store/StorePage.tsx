import { useEffect, useId, useState } from "react";
import { type Locale, type Phase, storeCopy } from "../copy";
import { useMediaListing } from "../useMediaListing";
import { browseApi } from "../packs/browseApi";
import type { PackSummary } from "../packs/model";
import {
  type CatalogFilter,
  type CatalogItem,
  type StoreTab,
  deriveStoreHistory,
  deriveStoreLibrary,
  deriveStoreWallet,
  filterCatalogItems,
  isStoreAuditionBlocked,
  PREVIEW_CATALOG_ITEMS,
} from "./storeModel";
import "./Store.css";

export function AbstractCover({ pattern }: { pattern: "waves" | "hexagons" | "rings" | "circuit" }) {
  if (pattern === "waves") {
    return (
      <svg className="store-card-cover-svg" viewBox="0 0 280 140" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
        <defs>
          <linearGradient id="waveGrad" x1="0" y1="0" x2="280" y2="140" gradientUnits="userSpaceOnUse">
            <stop stopColor="#1e293b" />
            <stop offset="0.5" stopColor="#0f172a" />
            <stop offset="1" stopColor="#312e81" />
          </linearGradient>
          <linearGradient id="lineGradA" x1="0" y1="70" x2="280" y2="70" gradientUnits="userSpaceOnUse">
            <stop stopColor="#38bdf8" stopOpacity="0.8" />
            <stop offset="1" stopColor="#818cf8" stopOpacity="0.4" />
          </linearGradient>
        </defs>
        <rect width="280" height="140" fill="url(#waveGrad)" />
        <path d="M0 70 Q 70 30, 140 70 T 280 70" stroke="url(#lineGradA)" strokeWidth="3" fill="none" />
        <path d="M0 85 Q 70 45, 140 85 T 280 85" stroke="#38bdf8" strokeWidth="1.5" strokeOpacity="0.5" fill="none" />
        <path d="M0 55 Q 70 15, 140 55 T 280 55" stroke="#818cf8" strokeWidth="1.5" strokeOpacity="0.5" fill="none" />
        <circle cx="140" cy="70" r="24" stroke="#38bdf8" strokeWidth="2" strokeOpacity="0.3" fill="none" />
      </svg>
    );
  }
  if (pattern === "hexagons") {
    return (
      <svg className="store-card-cover-svg" viewBox="0 0 280 140" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
        <defs>
          <linearGradient id="hexGrad" x1="0" y1="0" x2="280" y2="140" gradientUnits="userSpaceOnUse">
            <stop stopColor="#1c1917" />
            <stop offset="0.5" stopColor="#292524" />
            <stop offset="1" stopColor="#44403c" />
          </linearGradient>
        </defs>
        <rect width="280" height="140" fill="url(#hexGrad)" />
        <polygon points="140,40 165,55 165,85 140,100 115,85 115,55" stroke="#f59e0b" strokeWidth="2" fill="none" strokeOpacity="0.7" />
        <polygon points="80,55 105,70 105,100 80,115 55,100 55,70" stroke="#f59e0b" strokeWidth="1.5" fill="none" strokeOpacity="0.4" />
        <polygon points="200,55 225,70 225,100 200,115 175,100 175,70" stroke="#f59e0b" strokeWidth="1.5" fill="none" strokeOpacity="0.4" />
      </svg>
    );
  }
  if (pattern === "rings") {
    return (
      <svg className="store-card-cover-svg" viewBox="0 0 280 140" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
        <defs>
          <linearGradient id="ringGrad" x1="0" y1="0" x2="280" y2="140" gradientUnits="userSpaceOnUse">
            <stop stopColor="#064e3b" />
            <stop offset="0.7" stopColor="#022c22" />
            <stop offset="1" stopColor="#0f172a" />
          </linearGradient>
        </defs>
        <rect width="280" height="140" fill="url(#ringGrad)" />
        <circle cx="140" cy="70" r="15" stroke="#34d399" strokeWidth="2" strokeOpacity="0.8" fill="none" />
        <circle cx="140" cy="70" r="32" stroke="#34d399" strokeWidth="1.5" strokeOpacity="0.6" strokeDasharray="4 4" fill="none" />
        <circle cx="140" cy="70" r="50" stroke="#10b981" strokeWidth="1" strokeOpacity="0.4" fill="none" />
        <circle cx="140" cy="70" r="68" stroke="#059669" strokeWidth="1" strokeOpacity="0.25" fill="none" />
      </svg>
    );
  }
  return (
    <svg className="store-card-cover-svg" viewBox="0 0 280 140" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
      <defs>
        <linearGradient id="circGrad" x1="0" y1="0" x2="280" y2="140" gradientUnits="userSpaceOnUse">
          <stop stopColor="#311042" />
          <stop offset="0.5" stopColor="#1a0c2e" />
          <stop offset="1" stopColor="#0d0417" />
        </linearGradient>
      </defs>
      <rect width="280" height="140" fill="url(#circGrad)" />
      <path d="M30 40 H 100 L 130 70 H 250" stroke="#c084fc" strokeWidth="2" strokeOpacity="0.7" fill="none" />
      <path d="M40 100 H 110 L 140 70 H 220" stroke="#a855f7" strokeWidth="1.5" strokeOpacity="0.5" fill="none" />
      <circle cx="130" cy="70" r="4" fill="#e9d5ff" />
      <circle cx="100" cy="40" r="3" fill="#c084fc" />
      <circle cx="110" cy="100" r="3" fill="#a855f7" />
    </svg>
  );
}

export function StorePage({
  locale = "th",
  disabled = false,
  phase = null,
  onBackToPacks,
}: {
  locale?: Locale;
  disabled?: boolean;
  phase?: Phase | null;
  onBackToPacks?: () => void;
}) {
  const text = storeCopy(locale);
  const id = useId();
  const [tab, setTab] = useState<StoreTab>("catalog");
  const [filter, setFilter] = useState<CatalogFilter>("all");
  const [selectedPack, setSelectedPack] = useState<CatalogItem | null>(null);
  const [auditionNotice, setAuditionNotice] = useState<string | null>(null);

  // Real installed packs listing from local machine
  const listing = useMediaListing();
  const [summaries, setSummaries] = useState<PackSummary[] | null>(null);

  useEffect(() => {
    let current = true;
    void browseApi.summaries().then(result => {
      if (current && result) {
        setSummaries(result);
      }
    });
    return () => { current = false; };
  }, [listing.listing]);

  const realLibrary = deriveStoreLibrary(summaries);
  const wallet = deriveStoreWallet(locale);
  const history = deriveStoreHistory();
  const filteredCatalog = filterCatalogItems(PREVIEW_CATALOG_ITEMS, filter);
  const isMatchAuditionBlocked = isStoreAuditionBlocked(phase);

  const handleAudition = (sampleName: string) => {
    if (disabled) {
      setAuditionNotice(locale === "th" ? "ระบบเสียงไม่พร้อมใช้งาน" : "Audio unavailable");
      return;
    }
    if (isMatchAuditionBlocked) {
      setAuditionNotice(text.matchBlockedAudition);
      return;
    }
    setAuditionNotice(`${text.listenSample}: ${sampleName}`);
  };

  const openDetail = (item: CatalogItem) => {
    setSelectedPack(item);
    setTab("detail");
    setAuditionNotice(null);
  };

  return (
    <section className="shell-screen store-screen" aria-labelledby="store-heading">
      {/* 1. Design Preview Banner Across the Store */}
      <div className="store-preview-banner" role="alert">
        <div className="store-preview-banner-text">
          <span className="store-preview-tag">{text.previewBadge}</span>
          <span>{text.previewBanner}</span>
        </div>
        <span className="store-preview-banner-note">({text.previewBannerEn})</span>
      </div>

      {/* 2. Store Header & Navigation */}
      <header className="store-header-bar">
        <div className="store-nav-tabs" role="tablist" aria-label="Store navigation">
          {onBackToPacks && (
            <button type="button" className="store-back-button" onClick={onBackToPacks}>
              ← {text.backToPacks}
            </button>
          )}
          <button
            type="button"
            role="tab"
            aria-selected={tab === "catalog"}
            className="store-nav-tab"
            onClick={() => { setTab("catalog"); setAuditionNotice(null); }}
          >
            {text.tabs.catalog}
          </button>
          {selectedPack && (
            <button
              type="button"
              role="tab"
              aria-selected={tab === "detail"}
              className="store-nav-tab"
              onClick={() => { setTab("detail"); setAuditionNotice(null); }}
            >
              {text.tabs.detail}
            </button>
          )}
          <button
            type="button"
            role="tab"
            aria-selected={tab === "wallet"}
            className="store-nav-tab"
            onClick={() => { setTab("wallet"); setAuditionNotice(null); }}
          >
            {text.tabs.wallet}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === "library"}
            className="store-nav-tab"
            onClick={() => { setTab("library"); setAuditionNotice(null); }}
          >
            {text.tabs.library} ({realLibrary.length})
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === "history"}
            className="store-nav-tab"
            onClick={() => { setTab("history"); setAuditionNotice(null); }}
          >
            {text.tabs.history}
          </button>
        </div>

        <div className="store-wallet-slot">
          <div className="store-balance-badge" title={text.balanceLabel}>
            <span>{text.balanceLabel}:</span>
            <strong>{text.balanceDisplay}</strong>
          </div>
          <button type="button" className="store-topup-button" disabled>
            {text.topUpButton}
          </button>
        </div>
      </header>

      {/* 3. Single Scrollable Content Area */}
      <div className="store-content-scroll" tabIndex={0}>
        {auditionNotice && (
          <div className="store-audition-notice" role="status">
            {auditionNotice}
          </div>
        )}

        {/* View 1: Catalog */}
        {tab === "catalog" && (
          <div className="store-catalog-view">
            <div className="store-filter-toolbar">
              <div className="store-filter-group" role="group" aria-label="Catalog filters">
                <button
                  type="button"
                  className="store-filter-pill"
                  data-active={filter === "all"}
                  onClick={() => setFilter("all")}
                >
                  {text.filters.all}
                </button>
                <button
                  type="button"
                  className="store-filter-pill"
                  data-active={filter === "announcer"}
                  onClick={() => setFilter("announcer")}
                >
                  {text.filters.announcer}
                </button>
                <button
                  type="button"
                  className="store-filter-pill"
                  data-active={filter === "credits"}
                  onClick={() => setFilter("credits")}
                >
                  {text.filters.credits}
                </button>
              </div>
              <span className="store-items-count">
                {filteredCatalog.length} {locale === "th" ? "รายการตัวอย่าง" : "preview items"}
              </span>
            </div>

            <div className="store-catalog-grid">
              {filteredCatalog.map(item => (
                <article key={item.id} className="store-card" aria-label={item.name}>
                  <div className="store-card-cover-container">
                    <AbstractCover pattern={item.coverPattern} />
                    <span className="store-card-badge">{item.badge}</span>
                    <span className="store-card-category-badge">
                      {item.category === "announcer" ? text.filters.announcer : text.filters.credits}
                    </span>
                  </div>

                  <div className="store-card-body">
                    <h3 className="store-card-title">{item.name}</h3>
                    <div className="store-card-meta">
                      <span>{text.authorLabel}: {item.voiceAuthor}</span>
                      <span>{text.languageLabel}: {item.language}</span>
                    </div>
                    <p className="store-card-desc">{item.description}</p>

                    <div className="store-card-footer">
                      <div className="store-card-price-row">
                        <span className="store-card-price">{item.priceDisplay}</span>
                      </div>

                      <div className="store-card-actions">
                        <button
                          type="button"
                          className="store-audition-button"
                          disabled={disabled || isMatchAuditionBlocked}
                          onClick={() => handleAudition(item.samples[0]?.name ?? item.name)}
                          title={isMatchAuditionBlocked ? text.matchBlockedAudition : text.listenSample}
                        >
                          ▶ {text.listenSample}
                        </button>
                        <button
                          type="button"
                          className="store-buy-button"
                          disabled={item.buyDisabled}
                        >
                          {item.buyLabel}
                        </button>
                      </div>

                      <button
                        type="button"
                        className="store-detail-link-btn"
                        onClick={() => openDetail(item)}
                      >
                        {text.tabs.detail} →
                      </button>
                    </div>
                  </div>
                </article>
              ))}
            </div>
          </div>
        )}

        {/* View 2: Pack Detail */}
        {tab === "detail" && selectedPack && (
          <div className="store-detail-view">
            <button
              type="button"
              className="store-back-button"
              style={{ alignSelf: "flex-start" }}
              onClick={() => setTab("catalog")}
            >
              ← {text.backToCatalog}
            </button>

            <div className="store-detail-header">
              <div className="store-detail-cover">
                <AbstractCover pattern={selectedPack.coverPattern} />
                <span className="store-card-badge">{selectedPack.badge}</span>
              </div>
              <div className="store-detail-intro">
                <div className="store-detail-title-row">
                  <h2 className="store-detail-title">{selectedPack.name}</h2>
                  <span className="store-preview-tag">{selectedPack.badge}</span>
                </div>
                <div className="store-detail-author">
                  <span>{text.authorLabel}: <strong>{selectedPack.voiceAuthor}</strong></span> · 
                  <span> {text.languageLabel}: {selectedPack.language}</span>
                </div>
                <p className="store-detail-desc">{selectedPack.description}</p>

                <div className="store-detail-buy-box">
                  <span className="store-detail-price">{selectedPack.priceDisplay}</span>
                  <button
                    type="button"
                    className="store-buy-button"
                    style={{ minWidth: "120px", padding: "8px 16px" }}
                    disabled={selectedPack.buyDisabled}
                  >
                    {selectedPack.buyLabel}
                  </button>
                </div>
              </div>
            </div>

            {/* Samples Section */}
            <div className="store-detail-section">
              <h3>{text.sampleListTitle}</h3>
              <ul className="store-sample-list">
                {selectedPack.samples.map(sample => (
                  <li key={sample.id} className="store-sample-item">
                    <div className="store-sample-info">
                      <span>{sample.name}</span>
                      <span className="store-sample-duration">({sample.durationSec}s)</span>
                    </div>
                    <button
                      type="button"
                      className="store-audition-button"
                      disabled={disabled || isMatchAuditionBlocked}
                      onClick={() => handleAudition(sample.name)}
                    >
                      ▶ {text.listenSample}
                    </button>
                  </li>
                ))}
              </ul>
            </div>

            {/* Compatibility Section */}
            <div className="store-detail-section">
              <h3>{text.compatibilityTitle}</h3>
              <div className="store-compat-list">
                <div className="store-compat-row">
                  <span className="store-compat-label">{text.compatibilityTitle}</span>
                  <span className="store-compat-value">{selectedPack.compatibility.kohborVersion}</span>
                </div>
                <div className="store-compat-row">
                  <span className="store-compat-label">{text.signatureStatus}</span>
                  <span className="store-compat-value">{selectedPack.compatibility.signatureStatus}</span>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* View 3: Wallet / Kohbor Credits */}
        {tab === "wallet" && (
          <div className="store-wallet-view">
            <div className="store-wallet-balance-card">
              <h2>{wallet.balanceLabel}</h2>
              <div className="store-wallet-large-balance">{wallet.balance}</div>
            </div>

            <div className="store-wallet-section">
              <h3 style={{ margin: "0 0 12px 0", fontSize: "16px" }}>{text.walletTopUpTitle}</h3>
              <div className="store-topup-grid">
                {wallet.bundles.map(bundle => (
                  <div key={bundle.id} className="store-topup-card">
                    <span className="store-topup-thb">{bundle.label}</span>
                    <span className="store-topup-credits">{bundle.creditsLabel}</span>
                    <button type="button" className="store-topup-button" disabled={bundle.disabled}>
                      {bundle.buttonLabel}
                    </button>
                  </div>
                ))}
              </div>
            </div>

            <div className="store-wallet-explainer">
              <h3>{wallet.creditsUsageTitle}</h3>
              <p>{wallet.creditsUsageBody}</p>
            </div>
          </div>
        )}

        {/* View 4: Library (Owned - Real Data Only) */}
        {tab === "library" && (
          <div className="store-library-view">
            <div className="store-library-header">
              <h2>{text.libraryTitle}</h2>
              <p>{text.librarySubtitle}</p>
            </div>

            {realLibrary.length === 0 ? (
              <div className="store-history-view">
                <div className="store-empty-history-card">
                  <h3>{text.libraryEmpty}</h3>
                  <p>{text.librarySubtitle}</p>
                </div>
              </div>
            ) : (
              <div className="store-library-grid">
                {realLibrary.map(pack => (
                  <div key={pack.installId} className="store-library-card">
                    <div className="store-library-card-header">
                      <span className="store-library-card-title">{pack.name}</span>
                      {pack.status === "active" ? (
                        <span className="store-active-chip">{text.libraryActive}</span>
                      ) : (
                        <span className="store-installed-chip">{text.libraryInstalledBadge}</span>
                      )}
                    </div>
                    <div className="store-library-meta">
                      <span>{pack.revision}</span> · 
                      <span> {pack.eventCount} {locale === "th" ? "เหตุการณ์" : "events"}</span> · 
                      <span> {pack.variantCount} {locale === "th" ? "เสียง" : "voices"}</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* View 5: Purchase History (Honest Empty State) */}
        {tab === "history" && (
          <div className="store-history-view">
            <div className="store-empty-history-card">
              <h3>{text.historyEmptyTitle}</h3>
              <p>{text.historyEmptyBody}</p>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
