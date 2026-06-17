use std::fmt;

use super::{Result, SemanticScholarError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entity {
    Papers,
    Authors,
}

impl Entity {
    pub fn path_segment(&self) -> &'static str {
        match self {
            Entity::Papers => "paper",
            Entity::Authors => "author",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "papers" | "paper" | "works" | "work" => Ok(Entity::Papers),
            "authors" | "author" => Ok(Entity::Authors),
            other => Err(SemanticScholarError::UnknownEntity(other.to_string())),
        }
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path_segment())
    }
}

/// Infer entity type and S2 path id from a user-supplied id.
/// Returns `(entity, s2_path_id)` where `s2_path_id` is the form S2's path accepts.
pub fn infer_entity(id: &str) -> Result<(Entity, String)> {
    let id = id.trim();

    // Store lowercase prefix forms → translate to S2 native path forms (Papers)
    if let Some(bare) = id.strip_prefix("doi:") {
        return Ok((Entity::Papers, format!("DOI:{bare}")));
    }
    if let Some(bare) = id.strip_prefix("corpusid:") {
        return Ok((Entity::Papers, format!("CorpusId:{bare}")));
    }
    if let Some(bare) = id.strip_prefix("arxiv:") {
        return Ok((Entity::Papers, format!("ARXIV:{bare}")));
    }
    if let Some(bare) = id.strip_prefix("mag:") {
        return Ok((Entity::Papers, format!("MAG:{bare}")));
    }
    if let Some(bare) = id.strip_prefix("pmid:") {
        return Ok((Entity::Papers, format!("PMID:{bare}")));
    }
    if let Some(bare) = id.strip_prefix("pmcid:") {
        return Ok((Entity::Papers, format!("PMCID:{bare}")));
    }
    // s2:<paperId> → bare paperId
    if let Some(bare) = id.strip_prefix("s2:") {
        return Ok((Entity::Papers, bare.to_string()));
    }

    // s2author:<authorId> → Authors
    if let Some(bare) = id.strip_prefix("s2author:") {
        return Ok((Entity::Authors, bare.to_string()));
    }

    // S2 native forms passed through unchanged (Papers)
    if id.starts_with("DOI:")
        || id.starts_with("ARXIV:")
        || id.starts_with("CorpusId:")
        || id.starts_with("MAG:")
        || id.starts_with("PMID:")
        || id.starts_with("PMCID:")
        || id.starts_with("ACL:")
        || id.starts_with("DBLP:")
        || id.starts_with("URL:")
    {
        return Ok((Entity::Papers, id.to_string()));
    }

    // A bare 40-char lowercase-hex string → Papers (S2 paperId)
    if id.len() == 40
        && id.chars().all(|c| c.is_ascii_digit() || matches!(c, 'a'..='f'))
    {
        return Ok((Entity::Papers, id.to_string()));
    }

    Err(SemanticScholarError::InferFailed(format!(
        "{id} — use a prefix: doi:, arxiv:, corpusid:, mag:, pmid:, pmcid:, s2:, s2author: or a 40-char hex S2 paperId"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_doi_prefix() {
        let (e, id) = infer_entity("doi:10.7717/peerj.4375").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "DOI:10.7717/peerj.4375");
    }

    #[test]
    fn infer_corpusid() {
        let (e, id) = infer_entity("corpusid:12345").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "CorpusId:12345");
    }

    #[test]
    fn infer_arxiv() {
        let (e, id) = infer_entity("arxiv:2301.07041").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "ARXIV:2301.07041");
    }

    #[test]
    fn infer_mag() {
        let (e, id) = infer_entity("mag:2741809807").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "MAG:2741809807");
    }

    #[test]
    fn infer_pmid() {
        let (e, id) = infer_entity("pmid:29456894").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "PMID:29456894");
    }

    #[test]
    fn infer_pmcid() {
        let (e, id) = infer_entity("pmcid:PMC5045003").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "PMCID:PMC5045003");
    }

    #[test]
    fn infer_s2_prefix() {
        let (e, id) = infer_entity("s2:649def34f8be52c8b66281af98ae884c09aef38b").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "649def34f8be52c8b66281af98ae884c09aef38b");
    }

    #[test]
    fn infer_s2author() {
        let (e, id) = infer_entity("s2author:1741101").unwrap();
        assert_eq!(e, Entity::Authors);
        assert_eq!(id, "1741101");
    }

    #[test]
    fn infer_s2_native_doi() {
        let (e, id) = infer_entity("DOI:10.7717/peerj.4375").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "DOI:10.7717/peerj.4375");
    }

    #[test]
    fn infer_s2_native_arxiv() {
        let (e, id) = infer_entity("ARXIV:2301.07041").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "ARXIV:2301.07041");
    }

    #[test]
    fn infer_s2_native_corpusid() {
        let (e, id) = infer_entity("CorpusId:12345").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "CorpusId:12345");
    }

    #[test]
    fn infer_s2_native_mag() {
        let (e, id) = infer_entity("MAG:2741809807").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "MAG:2741809807");
    }

    #[test]
    fn infer_s2_native_acl() {
        let (e, id) = infer_entity("ACL:D15-1001").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "ACL:D15-1001");
    }

    #[test]
    fn infer_s2_native_dblp() {
        let (e, id) = infer_entity("DBLP:conf/emnlp/BerantCFL13").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "DBLP:conf/emnlp/BerantCFL13");
    }

    #[test]
    fn infer_bare_40hex() {
        let (e, id) = infer_entity("649def34f8be52c8b66281af98ae884c09aef38b").unwrap();
        assert_eq!(e, Entity::Papers);
        assert_eq!(id, "649def34f8be52c8b66281af98ae884c09aef38b");
    }

    #[test]
    fn infer_ambiguous_bare_numeric_fails() {
        assert!(infer_entity("12345").is_err());
    }

    #[test]
    fn infer_unknown_fails_loudly() {
        let err = infer_entity("unknown-form").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown-form"), "error should include the bad id: {msg}");
    }

    #[test]
    fn infer_bare_uppercase_hex_not_matched() {
        // 40-char uppercase hex should NOT match (S2 paperIds are lowercase hex)
        assert!(infer_entity("649DEF34F8BE52C8B66281AF98AE884C09AEF38B").is_err());
    }
}
