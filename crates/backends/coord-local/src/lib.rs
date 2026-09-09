//! No-op `Coordinator`, `SystemClock`, and `UuidGen` for single-threaded (local) use.
//!
//! In single-threaded operation, the alias UNIQUE constraint in the MetadataStore is the
//! real serialization point; `with_lock` is a no-op here. In Phase 5 this seam is bound
//! to a Cloudflare Durable Object.
//!
//! `Clock` and `IdGen` live here because they have no infra dependencies (just `std` +
//! `uuid`) and are used by tests alongside the no-op coordinator.

use async_trait::async_trait;
use braincrawl_core::{
    traits::{Clock, Coordinator, IdGen, LockGuard},
    types::{CanonicalId, DomainError},
};

// ---------------------------------------------------------------------------
// Coordinator
// ---------------------------------------------------------------------------

pub struct LocalCoordinator;

#[async_trait(?Send)]
impl Coordinator for LocalCoordinator {
    async fn with_lock(&self, _key: &str) -> Result<LockGuard, DomainError> {
        Ok(LockGuard)
    }
}

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

/// Wall-clock time via `std::time::SystemTime`, formatted as RFC 3339 UTC.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        secs_to_rfc3339(secs)
    }
}

fn secs_to_rfc3339(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let (y, mo, d) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, m, s)
}

fn days_to_ymd(mut days: u64) -> (u32, u32, u32) {
    let mut year = 1970u32;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        year += 1;
    }
    let months: [u32; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    for &dm in &months {
        if days < dm as u64 {
            break;
        }
        days -= dm as u64;
        month += 1;
    }
    (year, month, days as u32 + 1)
}

fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

// ---------------------------------------------------------------------------
// IdGen
// ---------------------------------------------------------------------------

/// UUID v4 generator.
pub struct UuidGen;

impl IdGen for UuidGen {
    fn new_guid(&self) -> CanonicalId {
        CanonicalId(uuid::Uuid::new_v4().to_string())
    }
}
