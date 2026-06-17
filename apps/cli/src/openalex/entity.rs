use std::fmt;

use super::{OpenAlexError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entity {
    Works,
    Authors,
    Sources,
    Institutions,
    Topics,
    Keywords,
    Publishers,
    Funders,
    Concepts,
}

impl Entity {
    pub fn path_segment(&self) -> &'static str {
        match self {
            Entity::Works => "works",
            Entity::Authors => "authors",
            Entity::Sources => "sources",
            Entity::Institutions => "institutions",
            Entity::Topics => "topics",
            Entity::Keywords => "keywords",
            Entity::Publishers => "publishers",
            Entity::Funders => "funders",
            Entity::Concepts => "concepts",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "works" | "work" => Ok(Entity::Works),
            "authors" | "author" => Ok(Entity::Authors),
            "sources" | "source" | "journals" | "journal" => Ok(Entity::Sources),
            "institutions" | "institution" => Ok(Entity::Institutions),
            "topics" | "topic" => Ok(Entity::Topics),
            "keywords" | "keyword" => Ok(Entity::Keywords),
            "publishers" | "publisher" => Ok(Entity::Publishers),
            "funders" | "funder" => Ok(Entity::Funders),
            "concepts" | "concept" => Ok(Entity::Concepts),
            other => Err(OpenAlexError::UnknownEntity(other.to_string())),
        }
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path_segment())
    }
}

/// Infer entity type and normalised ID string from an OpenAlex or external ID.
/// Returns `(entity, id_for_url_path)`.
pub fn infer_entity(id: &str) -> Result<(Entity, String)> {
    let id = id.trim();

    // Full OpenAlex URL — strip prefix and recurse
    if let Some(path) = id.strip_prefix("https://openalex.org/") {
        return infer_entity(path);
    }

    // OpenAlex canonical letter+digits prefixes
    if looks_like_openalex_id(id) {
        let first = id.chars().next().unwrap();
        let entity = match first {
            'W' => Entity::Works,
            'A' => Entity::Authors,
            'S' => Entity::Sources,
            'I' => Entity::Institutions,
            'T' => Entity::Topics,
            'P' => Entity::Publishers,
            'F' => Entity::Funders,
            'C' => Entity::Concepts,
            _ => unreachable!(),
        };
        return Ok((entity, id.to_string()));
    }

    // External ID prefixes for Works
    if id.starts_with("doi:")
        || id.starts_with("https://doi.org/")
        || id.starts_with("pmid:")
        || id.starts_with("pmcid:")
        || id.starts_with("mag:")
    {
        return Ok((Entity::Works, id.to_string()));
    }

    // External ID prefixes for Authors
    if id.starts_with("orcid:") || id.starts_with("https://orcid.org/") {
        return Ok((Entity::Authors, id.to_string()));
    }

    // External ID prefix for Sources
    if id.starts_with("issn:") {
        return Ok((Entity::Sources, id.to_string()));
    }

    // External ID prefix for Institutions
    if id.starts_with("ror:") {
        return Ok((Entity::Institutions, id.to_string()));
    }

    // Keywords are slugs — accept `keywords/<slug>` form explicitly
    if let Some(slug) = id.strip_prefix("keywords/") {
        return Ok((Entity::Keywords, slug.to_string()));
    }

    Err(OpenAlexError::InferFailed(id.to_string()))
}

fn looks_like_openalex_id(id: &str) -> bool {
    if id.len() < 2 {
        return false;
    }
    let first = id.chars().next().unwrap();
    if !matches!(first, 'W' | 'A' | 'S' | 'I' | 'T' | 'P' | 'F' | 'C') {
        return false;
    }
    id[1..].chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_w_prefix() {
        let (e, id) = infer_entity("W2741809807").unwrap();
        assert_eq!(e, Entity::Works);
        assert_eq!(id, "W2741809807");
    }

    #[test]
    fn infer_doi_prefix() {
        let (e, id) = infer_entity("doi:10.7717/peerj.4375").unwrap();
        assert_eq!(e, Entity::Works);
        assert_eq!(id, "doi:10.7717/peerj.4375");
    }

    #[test]
    fn infer_doi_url() {
        let (e, _) = infer_entity("https://doi.org/10.7717/peerj.4375").unwrap();
        assert_eq!(e, Entity::Works);
    }

    #[test]
    fn infer_pmid() {
        let (e, _) = infer_entity("pmid:29456894").unwrap();
        assert_eq!(e, Entity::Works);
    }

    #[test]
    fn infer_a_prefix() {
        let (e, _) = infer_entity("A5023888391").unwrap();
        assert_eq!(e, Entity::Authors);
    }

    #[test]
    fn infer_orcid_prefix() {
        let (e, _) = infer_entity("orcid:0000-0001-6187-6610").unwrap();
        assert_eq!(e, Entity::Authors);
    }

    #[test]
    fn infer_orcid_url() {
        let (e, _) = infer_entity("https://orcid.org/0000-0001-6187-6610").unwrap();
        assert_eq!(e, Entity::Authors);
    }

    #[test]
    fn infer_s_prefix() {
        let (e, _) = infer_entity("S1983995261").unwrap();
        assert_eq!(e, Entity::Sources);
    }

    #[test]
    fn infer_issn() {
        let (e, _) = infer_entity("issn:2041-1723").unwrap();
        assert_eq!(e, Entity::Sources);
    }

    #[test]
    fn infer_i_prefix() {
        let (e, _) = infer_entity("I27837315").unwrap();
        assert_eq!(e, Entity::Institutions);
    }

    #[test]
    fn infer_ror() {
        let (e, _) = infer_entity("ror:https://ror.org/00cvxb145").unwrap();
        assert_eq!(e, Entity::Institutions);
    }

    #[test]
    fn infer_t_prefix() {
        let (e, _) = infer_entity("T12419").unwrap();
        assert_eq!(e, Entity::Topics);
    }

    #[test]
    fn infer_p_prefix() {
        let (e, _) = infer_entity("P4310319965").unwrap();
        assert_eq!(e, Entity::Publishers);
    }

    #[test]
    fn infer_f_prefix() {
        let (e, _) = infer_entity("F4320306076").unwrap();
        assert_eq!(e, Entity::Funders);
    }

    #[test]
    fn infer_c_prefix() {
        let (e, _) = infer_entity("C41008148").unwrap();
        assert_eq!(e, Entity::Concepts);
    }

    #[test]
    fn infer_openalex_url() {
        let (e, id) = infer_entity("https://openalex.org/W2741809807").unwrap();
        assert_eq!(e, Entity::Works);
        assert_eq!(id, "W2741809807");
    }

    #[test]
    fn infer_keywords_slug() {
        let (e, id) = infer_entity("keywords/computer-science").unwrap();
        assert_eq!(e, Entity::Keywords);
        assert_eq!(id, "computer-science");
    }

    #[test]
    fn infer_unknown_fails() {
        assert!(infer_entity("unknown-id-form").is_err());
    }
}
