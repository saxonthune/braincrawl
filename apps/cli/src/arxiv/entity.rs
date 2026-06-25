use super::{ArxivError, Result};

/// Normalize a user-supplied arXiv id to a bare, version-stripped id accepted
/// by the arXiv API's `id_list` parameter.
///
/// Accepted forms:
///   arxiv:2301.07041 / arxiv:2301.07041v2    → 2301.07041
///   2301.07041 / 2301.07041v2                → 2301.07041
///   hep-th/9901001 (old-style)               → hep-th/9901001
///   arxiv:hep-th/9901001                     → hep-th/9901001
///   http(s)://arxiv.org/abs/2301.07041v2     → 2301.07041
pub fn infer_id(id: &str) -> Result<String> {
    let id = id.trim();

    // Strip http(s)://arxiv.org/abs/ prefix
    let bare = if let Some(rest) = id
        .strip_prefix("https://arxiv.org/abs/")
        .or_else(|| id.strip_prefix("http://arxiv.org/abs/"))
    {
        rest
    } else if let Some(rest) = id.strip_prefix("arxiv:") {
        rest
    } else {
        id
    };

    if bare.is_empty() {
        return Err(ArxivError::InferFailed(format!(
            "{id} — accepted forms: arxiv:<id>, bare numeric id (2301.07041), \
             old-style (hep-th/9901001), or https://arxiv.org/abs/<id>"
        )));
    }

    Ok(strip_version(bare))
}

/// Strip trailing version suffix `vN` (e.g. `2301.07041v2` → `2301.07041`).
/// Old-style ids like `hep-th/9901001` have no `v` suffix in typical use,
/// but we strip it if present.
pub fn strip_version_pub(id: &str) -> String {
    strip_version(id)
}

fn strip_version(id: &str) -> String {
    // Find the last occurrence of 'v' followed only by digits
    if let Some(pos) = id.rfind('v') {
        let suffix = &id[pos + 1..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return id[..pos].to_string();
        }
    }
    id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arxiv_prefix_bare() {
        assert_eq!(infer_id("arxiv:2301.07041").unwrap(), "2301.07041");
    }

    #[test]
    fn arxiv_prefix_versioned() {
        assert_eq!(infer_id("arxiv:2301.07041v2").unwrap(), "2301.07041");
    }

    #[test]
    fn bare_id() {
        assert_eq!(infer_id("2301.07041").unwrap(), "2301.07041");
    }

    #[test]
    fn bare_versioned() {
        assert_eq!(infer_id("2301.07041v2").unwrap(), "2301.07041");
    }

    #[test]
    fn old_style() {
        assert_eq!(infer_id("hep-th/9901001").unwrap(), "hep-th/9901001");
    }

    #[test]
    fn arxiv_prefix_old_style() {
        assert_eq!(infer_id("arxiv:hep-th/9901001").unwrap(), "hep-th/9901001");
    }

    #[test]
    fn full_url_https() {
        assert_eq!(
            infer_id("https://arxiv.org/abs/2301.07041v2").unwrap(),
            "2301.07041"
        );
    }

    #[test]
    fn full_url_http() {
        assert_eq!(
            infer_id("http://arxiv.org/abs/2301.07041v2").unwrap(),
            "2301.07041"
        );
    }

    #[test]
    fn strip_version_helper() {
        assert_eq!(strip_version("2301.07041v2"), "2301.07041");
        assert_eq!(strip_version("2301.07041"), "2301.07041");
        assert_eq!(strip_version("hep-th/9901001"), "hep-th/9901001");
    }
}
