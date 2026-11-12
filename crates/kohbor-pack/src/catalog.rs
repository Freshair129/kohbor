//! The part of contracts/event-catalog.json that package admission needs.
//! The caller loads the JSON; this crate does no file I/O for the catalog.

use serde_json::Value;

/// MP-05 taxonomy version `MAJOR.MINOR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaxonomyVersion {
    pub major: u32,
    pub minor: u32,
}

impl TaxonomyVersion {
    /// Canonical grammar only: two unsigned 32-bit ASCII integers separated by one
    /// dot; no signs, whitespace, suffixes, patch part or leading zeros (except the
    /// literal `0`). Anything else is `None`.
    pub fn parse(text: &str) -> Option<TaxonomyVersion> {
        let (major_text, minor_text) = text.split_once('.')?;
        if major_text.is_empty() || minor_text.is_empty() || minor_text.contains('.') {
            return None;
        }
        if !canonical_decimal(major_text) || !canonical_decimal(minor_text) {
            return None;
        }
        Some(TaxonomyVersion {
            major: major_text.parse().ok()?,
            minor: minor_text.parse().ok()?,
        })
    }
}

fn canonical_decimal(text: &str) -> bool {
    text.bytes().all(|byte| byte.is_ascii_digit()) && (text.len() == 1 || !text.starts_with('0'))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEvent {
    pub id: String,
    /// `media_mappable` in the catalog; a binding to a non-mappable event is inert.
    pub media_mappable: bool,
    /// Inclusive `tier_range`, when the event has counted tiers.
    pub tier_range: Option<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub taxonomy: TaxonomyVersion,
    pub events: Vec<CatalogEvent>,
}

impl Catalog {
    /// Read `taxonomy_version` and `events[]` (`id`, `media_mappable`, `tier_range`)
    /// from the parsed catalog document. Retired or non-active events are not
    /// listed. `None` when the document does not have the expected shape.
    pub fn from_json(doc: &Value) -> Option<Catalog> {
        let root = doc.as_object()?;
        let taxonomy = TaxonomyVersion::parse(root.get("taxonomy_version")?.as_str()?)?;
        let source_events = root.get("events")?.as_array()?;
        if source_events.is_empty() || source_events.len() > 128 {
            return None;
        }
        let mut events = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for source in source_events {
            let object = source.as_object()?;
            let id = object.get("id")?.as_str()?;
            if !valid_event_id(id) || !seen_ids.insert(id) {
                return None;
            }
            let status = object.get("status")?.as_str()?;
            if !matches!(
                status,
                "active" | "proposed_unavailable" | "test_only" | "deprecated"
            ) {
                return None;
            }
            let media_mappable = object.get("media_mappable")?.as_bool()?;
            let tier_range = match object.get("tier_range") {
                None => None,
                Some(value) => {
                    let range = value.as_object()?;
                    if range.len() != 2 || range.keys().any(|key| key != "min" && key != "max") {
                        return None;
                    }
                    let min = range.get("min")?.as_u64()?;
                    let max = range.get("max")?.as_u64()?;
                    let min = u32::try_from(min).ok()?;
                    let max = u32::try_from(max).ok()?;
                    if min == 0 || max < min || max > 100 {
                        return None;
                    }
                    Some((min, max))
                }
            };

            if status == "active" {
                events.push(CatalogEvent {
                    id: id.to_owned(),
                    media_mappable,
                    tier_range,
                });
            }
        }

        Some(Catalog { taxonomy, events })
    }

    pub fn event(&self, id: &str) -> Option<&CatalogEvent> {
        self.events.iter().find(|event| event.id == id)
    }
}

fn valid_event_id(text: &str) -> bool {
    let mut bytes = text.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && text.len() <= 80
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}
