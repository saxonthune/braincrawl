//! `IdResolver` backed by Cloudflare KV.

#![allow(dead_code)]

use async_trait::async_trait;
use braincrawl_core::{
    ports::IdResolver,
    types::{DomainError, WorkId},
};

pub struct KvResolver;

#[async_trait(?Send)]
impl IdResolver for KvResolver {
    async fn resolve(&self, _namespace: &str, _value: &str) -> Result<Option<WorkId>, DomainError> {
        todo!("implement against Cloudflare KV")
    }

    async fn remember(
        &self,
        _canonical: &WorkId,
        _namespace: &str,
        _value: &str,
    ) -> Result<(), DomainError> {
        todo!("implement against Cloudflare KV")
    }
}
