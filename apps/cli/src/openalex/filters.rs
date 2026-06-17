use super::{OpenAlexError, Result};
use super::entity::Entity;

/// A parsed key:value filter expression.
pub struct KV {
    pub key: String,
    pub value: String,
}

impl KV {
    /// Parse `key:value` splitting at the first colon. Values may contain colons (e.g. URLs).
    pub fn parse(s: &str) -> std::result::Result<Self, String> {
        match s.split_once(':') {
            Some((k, v)) => Ok(KV { key: k.to_string(), value: v.to_string() }),
            None => Err(format!("invalid filter (expected key:value): {s}")),
        }
    }
}

/// Validate filter keys against a static per-entity allowlist and return the
/// encoded `filter=` string value on success, or an error listing bad keys.
pub fn validate_filters(entity: Entity, kvs: &[KV]) -> Result<String> {
    let allowed = allowed_keys(entity);
    let bad: Vec<&str> = kvs
        .iter()
        .filter(|kv| !allowed.contains(&kv.key.as_str()))
        .map(|kv| kv.key.as_str())
        .collect();
    if !bad.is_empty() {
        return Err(OpenAlexError::BadFilter {
            entity: entity.to_string(),
            keys: bad.join(", "),
        });
    }
    let filter_str = kvs
        .iter()
        .map(|kv| format!("{}:{}", kv.key, kv.value))
        .collect::<Vec<_>>()
        .join(",");
    Ok(filter_str)
}

fn allowed_keys(entity: Entity) -> &'static [&'static str] {
    match entity {
        Entity::Works => WORKS_FILTERS,
        Entity::Authors => AUTHORS_FILTERS,
        Entity::Sources => SOURCES_FILTERS,
        Entity::Institutions => INSTITUTIONS_FILTERS,
        Entity::Topics => TOPICS_FILTERS,
        Entity::Keywords => KEYWORDS_FILTERS,
        Entity::Publishers => PUBLISHERS_FILTERS,
        Entity::Funders => FUNDERS_FILTERS,
        Entity::Concepts => CONCEPTS_FILTERS,
    }
}

static WORKS_FILTERS: &[&str] = &[
    "abstract.search",
    "apc_list.currency",
    "apc_list.provenance",
    "apc_list.value",
    "apc_list.value_usd",
    "apc_paid.currency",
    "apc_paid.provenance",
    "apc_paid.value",
    "apc_paid.value_usd",
    "author.id",
    "author.orcid",
    "authorships.affiliations.institution_ids",
    "authorships.author.id",
    "authorships.author.orcid",
    "authorships.countries",
    "authorships.institutions.continent",
    "authorships.institutions.country_code",
    "authorships.institutions.id",
    "authorships.institutions.is_global_south",
    "authorships.institutions.lineage",
    "authorships.institutions.ror",
    "authorships.institutions.type",
    "authorships.is_corresponding",
    "authors_count",
    "best_oa_location.is_accepted",
    "best_oa_location.is_published",
    "best_oa_location.license",
    "best_oa_location.source.host_organization",
    "best_oa_location.source.id",
    "best_oa_location.source.is_in_doaj",
    "best_oa_location.source.issn",
    "best_oa_location.source.type",
    "best_oa_location.version",
    "best_open_version",
    "biblio.first_page",
    "biblio.issue",
    "biblio.last_page",
    "biblio.volume",
    "cited_by",
    "cited_by_count",
    "cites",
    "concept.id",
    "concepts.id",
    "concepts.wikidata",
    "concepts_count",
    "corresponding_author_ids",
    "corresponding_institution_ids",
    "countries_distinct_count",
    "default.search",
    "display_name.search",
    "doi",
    "from_created_date",
    "from_publication_date",
    "from_updated_date",
    "fulltext.search",
    "fulltext_origin",
    "fwci",
    "grants.award_id",
    "grants.funder",
    "has_abstract",
    "has_doi",
    "has_fulltext",
    "has_ngrams",
    "has_oa_accepted_or_published_version",
    "has_oa_submitted_version",
    "has_orcid",
    "has_pmcid",
    "has_pmid",
    "has_references",
    "ids.mag",
    "ids.openalex",
    "ids.pmcid",
    "ids.pmid",
    "indexed_in",
    "institutions.continent",
    "institutions.country_code",
    "institutions.id",
    "institutions.is_global_south",
    "institutions.ror",
    "institutions_distinct_count",
    "is_corresponding",
    "is_oa",
    "is_paratext",
    "is_retracted",
    "journal",
    "keywords.id",
    "keywords.keyword",
    "language",
    "locations.is_accepted",
    "locations.is_oa",
    "locations.is_published",
    "locations.license",
    "locations.source.host_institution_lineage",
    "locations.source.host_organization",
    "locations.source.id",
    "locations.source.is_core",
    "locations.source.is_in_doaj",
    "locations.source.issn",
    "locations.source.publisher_lineage",
    "locations.source.type",
    "locations.version",
    "locations_count",
    "mag",
    "mag_only",
    "oa_status",
    "open_access.any_repository_has_fulltext",
    "open_access.is_oa",
    "open_access.oa_status",
    "openalex",
    "pmid",
    "primary_location.is_accepted",
    "primary_location.is_oa",
    "primary_location.is_published",
    "primary_location.license",
    "primary_location.source.has_issn",
    "primary_location.source.host_organization",
    "primary_location.source.id",
    "primary_location.source.is_core",
    "primary_location.source.is_in_doaj",
    "primary_location.source.issn",
    "primary_location.source.publisher_lineage",
    "primary_location.source.type",
    "primary_location.version",
    "primary_topic.domain.id",
    "primary_topic.field.id",
    "primary_topic.id",
    "primary_topic.subfield.id",
    "publication_date",
    "publication_year",
    "raw_affiliation_strings.search",
    "related_to",
    "repository",
    "sustainable_development_goals.id",
    "title.search",
    "title_and_abstract.search",
    "to_created_date",
    "to_publication_date",
    "to_updated_date",
    "topics.domain.id",
    "topics.field.id",
    "topics.id",
    "topics.subfield.id",
    "type",
    "type_crossref",
    "version",
];

static AUTHORS_FILTERS: &[&str] = &[
    "affiliations.institution.country_code",
    "affiliations.institution.id",
    "affiliations.institution.lineage",
    "affiliations.institution.ror",
    "affiliations.institution.type",
    "cited_by_count",
    "concept.id",
    "concepts.id",
    "display_name",
    "from_created_date",
    "has_orcid",
    "id",
    "ids.openalex",
    "last_known_institutions.continent",
    "last_known_institutions.country_code",
    "last_known_institutions.id",
    "last_known_institutions.is_global_south",
    "last_known_institutions.lineage",
    "last_known_institutions.ror",
    "last_known_institutions.type",
    "openalex",
    "orcid",
    "scopus",
    "summary_stats.2yr_mean_citedness",
    "summary_stats.h_index",
    "summary_stats.i10_index",
    "to_created_date",
    "to_updated_date",
    "topic_share.id",
    "topics.id",
    "works_count",
    "x_concepts.id",
];

static SOURCES_FILTERS: &[&str] = &[
    "apc_prices.currency",
    "apc_prices.price",
    "apc_usd",
    "cited_by_count",
    "continent",
    "country_code",
    "default.search",
    "display_name.search",
    "has_issn",
    "host_organization",
    "host_organization_lineage",
    "ids.openalex",
    "is_core",
    "is_global_south",
    "is_in_doaj",
    "is_oa",
    "issn",
    "openalex",
    "publisher",
    "summary_stats.2yr_mean_citedness",
    "summary_stats.h_index",
    "summary_stats.i10_index",
    "type",
    "works_count",
    "x_concepts.id",
];

static INSTITUTIONS_FILTERS: &[&str] = &[
    "cited_by_count",
    "continent",
    "country_code",
    "default.search",
    "display_name.search",
    "has_ror",
    "is_global_south",
    "is_super_system",
    "lineage",
    "openalex",
    "repositories.host_organization",
    "repositories.host_organization_lineage",
    "repositories.id",
    "ror",
    "summary_stats.2yr_mean_citedness",
    "summary_stats.h_index",
    "summary_stats.i10_index",
    "type",
    "works_count",
    "x_concepts.id",
];

static TOPICS_FILTERS: &[&str] = &[
    "cited_by_count",
    "display_name",
    "domain.id",
    "field.id",
    "from_created_date",
    "id",
    "ids.openalex",
    "openalex",
    "subfield.id",
    "works_count",
];

static KEYWORDS_FILTERS: &[&str] = &[
    "cited_by_count",
    "display_name",
    "from_created_date",
    "id",
    "works_count",
];

static PUBLISHERS_FILTERS: &[&str] = &[
    "cited_by_count",
    "continent",
    "country_codes",
    "display_name",
    "from_created_date",
    "hierarchy_level",
    "ids.openalex",
    "ids.ror",
    "ids.wikidata",
    "lineage",
    "parent_publisher",
    "roles.id",
    "summary_stats.2yr_mean_citedness",
    "summary_stats.h_index",
    "summary_stats.i10_index",
    "works_count",
];

static FUNDERS_FILTERS: &[&str] = &[
    "awards_count",
    "cited_by_count",
    "continent",
    "country_code",
    "display_name",
    "from_created_date",
    "ids.crossref",
    "ids.doi",
    "ids.openalex",
    "ids.ror",
    "ids.wikidata",
    "is_global_south",
    "openalex",
    "roles.id",
    "ror",
    "summary_stats.2yr_mean_citedness",
    "summary_stats.h_index",
    "summary_stats.i10_index",
    "wikidata",
    "works_count",
];

static CONCEPTS_FILTERS: &[&str] = &[
    "ancestors.id",
    "cited_by_count",
    "display_name",
    "from_created_date",
    "has_wikidata",
    "ids.openalex",
    "level",
    "openalex_id",
    "summary_stats.2yr_mean_citedness",
    "summary_stats.h_index",
    "summary_stats.i10_index",
    "wikidata_id",
    "works_count",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_known_works_keys() {
        let kvs = vec![
            KV { key: "publication_year".into(), value: "2020".into() },
            KV { key: "is_oa".into(), value: "true".into() },
        ];
        let result = validate_filters(Entity::Works, &kvs).unwrap();
        assert_eq!(result, "publication_year:2020,is_oa:true");
    }

    #[test]
    fn validate_unknown_key_errors() {
        let kvs = vec![
            KV { key: "nonexistent_key".into(), value: "foo".into() },
        ];
        let err = validate_filters(Entity::Works, &kvs).unwrap_err();
        assert!(matches!(err, OpenAlexError::BadFilter { .. }));
    }

    #[test]
    fn validate_empty_filters() {
        let result = validate_filters(Entity::Authors, &[]).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn kv_parse_splits_at_first_colon() {
        let kv = KV::parse("institutions.country_code:us").unwrap();
        assert_eq!(kv.key, "institutions.country_code");
        assert_eq!(kv.value, "us");
    }

    #[test]
    fn kv_parse_value_with_colon() {
        // URL values contain colons; split at first colon only
        let kv = KV::parse("doi:https://doi.org/10.1/test").unwrap();
        assert_eq!(kv.key, "doi");
        assert_eq!(kv.value, "https://doi.org/10.1/test");
    }
}
