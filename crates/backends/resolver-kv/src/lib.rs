//! `IdResolver` backed by Cloudflare KV.
//!
//! KV is the hot-cache projection (doc02.01.01): D1's `alias` table remains
//! source of truth.  `resolve` checks KV before hitting D1; `remember` writes
//! the resolved canonical id into KV so subsequent resolves are cache hits.
//!
//! Key format: `"{scheme}:{value}"` — matches the alias identity used in
//! the rest of the system.

#![allow(dead_code)]

use async_trait::async_trait;
use braincrawl_core::{
    traits::IdResolver,
    types::{CanonicalId, DomainError},
};

fn be(e: impl std::fmt::Display) -> DomainError {
    DomainError::Backend(e.to_string())
}

// ── stub (no `cloudflare` feature) ───────────────────────────────────────────

#[cfg(not(feature = "cloudflare"))]
pub struct KvResolver;

#[cfg(not(feature = "cloudflare"))]
#[async_trait(?Send)]
impl IdResolver for KvResolver {
    async fn resolve(&self, _scheme: &str, _value: &str) -> Result<Option<CanonicalId>, DomainError> {
        Err(DomainError::Backend("KvResolver: cloudflare feature not enabled".into()))
    }
    async fn remember(&self, _canonical: &CanonicalId, _scheme: &str, _value: &str) -> Result<(), DomainError> {
        Err(DomainError::Backend("KvResolver: cloudflare feature not enabled".into()))
    }
}

// ── real impl (cloudflare feature — wasm32 only) ─────────────────────────────

#[cfg(feature = "cloudflare")]
pub struct KvResolver {
    kv: worker::kv::KvStore,
}

#[cfg(feature = "cloudflare")]
impl KvResolver {
    pub fn new(kv: worker::kv::KvStore) -> Self {
        Self { kv }
    }
}

#[cfg(feature = "cloudflare")]
#[async_trait(?Send)]
impl IdResolver for KvResolver {
    async fn resolve(
        &self,
        scheme: &str,
        value: &str,
    ) -> Result<Option<CanonicalId>, DomainError> {
        let key = format!("{scheme}:{value}");
        let text = self.kv.get(&key).text().await.map_err(be)?;
        Ok(text.map(CanonicalId))
    }

    async fn remember(
        &self,
        canonical: &CanonicalId,
        scheme: &str,
        value: &str,
    ) -> Result<(), DomainError> {
        let key = format!("{scheme}:{value}");
        self.kv
            .put(&key, canonical.0.as_str())
            .map_err(be)?
            .execute()
            .await
            .map_err(be)?;
        Ok(())
    }
}
