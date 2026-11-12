//! Trust domains (MEDIA-008, MP-09). Package integrity, publisher authenticity,
//! purchase entitlement and active selection are evaluated separately. Until
//! DEC-PACK-CRYPTO closes, the only grantable authority is `local_import`; the
//! signature step reports `Unavailable`.

use crate::descriptor::Descriptor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityClass {
    LocalImport,
}

impl AuthorityClass {
    /// `local_import`.
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthorityClass::LocalImport => "local_import",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublisherClaim {
    None,
    Unverified,
}

impl PublisherClaim {
    /// `none` / `unverified`.
    pub fn as_str(&self) -> &'static str {
        match self {
            PublisherClaim::None => "none",
            PublisherClaim::Unverified => "unverified",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    /// DEC-PACK-CRYPTO is open: no signature scheme, no trust roots.
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustAssessment {
    pub authority_class: AuthorityClass,
    pub publisher_claim: PublisherClaim,
    pub signature: SignatureStatus,
}

impl TrustAssessment {
    /// Always false until DEC-PACK-CRYPTO closes and a trust root is application-owned.
    pub fn publisher_trusted(&self) -> bool {
        false
    }

    /// Always false: local integrity never grants purchase entitlement.
    pub fn purchase_entitled(&self) -> bool {
        false
    }
}

/// `authority_class` is `LocalImport` for every descriptor. `publisher_claim` is
/// `Unverified` when `publisher_key_id` is present and `None` otherwise. Nothing
/// in the descriptor (key ids, display name, package id such as `store`/`system`)
/// changes `publisher_trusted()` or `purchase_entitled()`.
pub fn assess(descriptor: &Descriptor) -> TrustAssessment {
    TrustAssessment {
        authority_class: AuthorityClass::LocalImport,
        publisher_claim: if descriptor.publisher_key_id.is_some() {
            PublisherClaim::Unverified
        } else {
            PublisherClaim::None
        },
        signature: SignatureStatus::Unavailable,
    }
}
