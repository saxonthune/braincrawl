//! `IdResolver` backed by an in-memory HashMap (for tests).

use std::cell::RefCell;
use std::collections::HashMap;

use async_trait::async_trait;
use braincrawl_core::{
    traits::IdResolver,
    types::{CanonicalId, DomainError},
};

pub struct MemResolver {
    inner: RefCell<HashMap<(String, String), CanonicalId>>,
}

impl MemResolver {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(HashMap::new()),
        }
    }
}

impl Default for MemResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl IdResolver for MemResolver {
    async fn resolve(
        &self,
        scheme: &str,
        value: &str,
    ) -> Result<Option<CanonicalId>, DomainError> {
        Ok(self
            .inner
            .borrow()
            .get(&(scheme.to_string(), value.to_string()))
            .cloned())
    }

    async fn remember(
        &self,
        canonical: &CanonicalId,
        scheme: &str,
        value: &str,
    ) -> Result<(), DomainError> {
        self.inner
            .borrow_mut()
            .insert((scheme.to_string(), value.to_string()), canonical.clone());
        Ok(())
    }
}
