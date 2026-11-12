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
        todo!()
    }
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
        todo!()
    }

    pub fn event(&self, id: &str) -> Option<&CatalogEvent> {
        todo!()
    }
}
