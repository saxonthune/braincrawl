use sha2::{Digest, Sha256};

/// The tenant a token resolves to (L3 isolation key; unused downstream this task).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tenant(pub String);

/// Hex-encoded SHA-256 of the raw token. The raw token is never persisted.
pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Parse `Authorization: Bearer <token>` → the token, or None if absent/malformed.
pub fn parse_bearer(header: Option<&str>) -> Option<&str> {
    let h = header?;
    h.strip_prefix("Bearer ")
}

/// Resolution result a runtime maps to a status code.
pub enum AuthOutcome {
    /// → 200: token matched the allowlist.
    Authenticated(Tenant),
    /// → 401: missing or malformed Authorization header.
    Unauthenticated,
    /// → 403: well-formed bearer token not found in the allowlist.
    Forbidden,
}

/// Allowlist abstraction. Shaped so a KV-backed impl is a later drop-in.
pub trait Allowlist {
    fn lookup(&self, token_hash: &str) -> Option<Tenant>;
}

/// Single shared secret → one fixed tenant. Stores only the hash of the secret.
pub struct SharedSecret {
    expected_hash: String,
    tenant: Tenant,
}

impl SharedSecret {
    /// Hashes `secret` on construction; the raw secret is not retained.
    pub fn new(secret: &str, tenant: &str) -> Self {
        Self {
            expected_hash: hash_token(secret),
            tenant: Tenant(tenant.to_string()),
        }
    }
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // XOR-accumulate without short-circuiting to avoid timing leaks.
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

impl Allowlist for SharedSecret {
    fn lookup(&self, token_hash: &str) -> Option<Tenant> {
        if ct_eq(self.expected_hash.as_bytes(), token_hash.as_bytes()) {
            Some(self.tenant.clone())
        } else {
            None
        }
    }
}

/// Decide an outcome from the raw Authorization header value.
pub fn authorize(allowlist: &dyn Allowlist, header: Option<&str>) -> AuthOutcome {
    let token = match parse_bearer(header) {
        Some(t) => t,
        None => return AuthOutcome::Unauthenticated,
    };
    let hash = hash_token(token);
    match allowlist.lookup(&hash) {
        Some(tenant) => AuthOutcome::Authenticated(tenant),
        None => AuthOutcome::Forbidden,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bearer_happy() {
        assert_eq!(parse_bearer(Some("Bearer mytoken")), Some("mytoken"));
    }

    #[test]
    fn parse_bearer_empty() {
        assert_eq!(parse_bearer(None), None);
    }

    #[test]
    fn parse_bearer_wrong_scheme() {
        assert_eq!(parse_bearer(Some("Basic abc")), None);
    }

    #[test]
    fn parse_bearer_bare_token_no_space() {
        assert_eq!(parse_bearer(Some("Bearer")), None);
    }

    #[test]
    fn hash_token_is_stable_and_64_hex_chars() {
        let h = hash_token("hello");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        // stability: SHA-256("hello") is deterministic
        assert_eq!(hash_token("hello"), h);
        // different input → different hash
        assert_ne!(hash_token("world"), h);
    }

    #[test]
    fn authorize_authenticated() {
        let al = SharedSecret::new("secret", "default");
        match authorize(&al, Some("Bearer secret")) {
            AuthOutcome::Authenticated(t) => assert_eq!(t.0, "default"),
            _ => panic!("expected Authenticated"),
        }
    }

    #[test]
    fn authorize_unauthenticated_no_header() {
        let al = SharedSecret::new("secret", "default");
        assert!(matches!(authorize(&al, None), AuthOutcome::Unauthenticated));
    }

    #[test]
    fn authorize_unauthenticated_malformed() {
        let al = SharedSecret::new("secret", "default");
        assert!(matches!(
            authorize(&al, Some("Basic xyz")),
            AuthOutcome::Unauthenticated
        ));
    }

    #[test]
    fn authorize_forbidden_wrong_token() {
        let al = SharedSecret::new("secret", "default");
        assert!(matches!(
            authorize(&al, Some("Bearer wrongtoken")),
            AuthOutcome::Forbidden
        ));
    }
}
