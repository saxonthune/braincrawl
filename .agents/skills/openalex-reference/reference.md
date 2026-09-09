# OpenAlex API — Raw Contract Reference

Generated from a crawl of `developers.openalex.org/api-reference` (2026-06-16).
This is a contract dump for lookup, not a tutorial. Base URL: `https://api.openalex.org`.

> **Doc-source note.** `docs.openalex.org` now 301-redirects to `developers.openalex.org`.
> The authoritative machine-readable spec is
> `https://developers.openalex.org/api-reference/openapi.json` and the LLM index is
> `https://developers.openalex.org/llms.txt`. The developers-site OpenAPI summary is
> abbreviated and in a few places inaccurate; field/filter lists below were reconciled
> against the `ourresearch/openalex-docs` source. Fields/filters flagged "verify" were
> seen live or in legacy docs but not confirmed in the current spec.

---

## 1. Protocol (cross-cutting)

### Base URL & envelope
- Single host: `https://api.openalex.org`. JSON only.
- **List endpoints** return an envelope: `{ "meta": {...}, "results": [...], "group_by": [...] }`.
- **Single-entity endpoints** return the bare entity object (no envelope).

### `meta` object
| Field | Type | Meaning |
|---|---|---|
| `count` | integer | Total results matching the query |
| `db_response_time_ms` | integer | DB response time |
| `page` | integer | Current page |
| `per_page` | integer | Results per page |
| `next_cursor` | string | Cursor for next page (cursor paging) |
| `groups_count` | integer\|null | Number of groups when `group_by` used |
| `cost_usd` | number | Cost of this request in USD |

### Authentication (freemium api_key model)
- Security scheme: `type: apiKey`, `in: query`, `name: api_key`. Passed as `?api_key=YOUR_KEY`.
- No HTTP header auth defined. **No `mailto` / User-Agent / "polite pool"** in the current
  contract (the legacy polite-pool model was replaced by the api_key freemium model).
- Key registration: `https://openalex.org/settings/api`.

### Rate limits / pricing (current freemium model)
- Free daily budget **with** key: `$1.00/day`. Without key: `$0.01/day`.
- Throttle: > `100 requests/second` → `429`.
- Per-operation cost: singleton lookup = free; list+filter = `$0.0001`/call; search (`?search=`)
  = `$0.001`/call; PDF/content download = `$0.01`/call.
- Usage headers (docs, not in OpenAPI): `X-RateLimit-Limit`, `X-RateLimit-Remaining`,
  `X-RateLimit-Credits-Used`, `X-RateLimit-Reset` (seconds to midnight-UTC reset).

### Error responses
Codes: `200`, `400` (bad params), `403` (invalid ID format / illegal chars like `,` `&`),
`404` (not found), `429` (rate/budget). Shape: `{ "error": "string", "message": "string" }`.

### ID prefixes & external IDs
| Entity | OpenAlex prefix (example) | Canonical external ID | Other accepted |
|---|---|---|---|
| Works | `W2741809807` | DOI | PMID, PMCID, MAG |
| Authors | `A5023888391` | ORCID | — |
| Sources | `S1983995261` | ISSN-L | ISSN, MAG, Wikidata, Fatcat |
| Institutions | `I27837315` | ROR | MAG, Wikidata |
| Topics | `T12419` | — | Wikipedia |
| Keywords | slug, e.g. `keywords/computer-science` | — | — |
| Publishers | `P4310319965` | — | ROR, Wikidata |
| Funders | `F4320306076` | — | Crossref Funder ID, ROR, Wikidata, DOI |
| Concepts (deprecated) | `C41008148` | — | Wikidata |
| Countries | ISO-2 (`US`, `GB`) | — | — |
| Continents | Wikidata Q-ID (`Q49`) | — | — |

External-ID request forms:
```
DOI:    /works/https://doi.org/10.1234/example   OR  /works/doi:10.1234/example
PMID:   /works/pmid:29456894
PMCID:  /works/pmcid:PMC5045003
MAG:    /works/mag:2741809807
ORCID:  /authors/https://orcid.org/0000-0001-6187-6610   OR  /authors/orcid:0000-...
ROR:    /institutions/ror:https://ror.org/00cvxb145
ISSN:   /sources/issn:2041-1723
```
Bulk external-ID lookup via OR filter (max 50): `/works?filter=doi:10.1/a|10.1/b`.

### Deprecations
- `/concepts` → use `/topics`. `host_venue` field → `primary_location`. `has_ngrams` filter →
  `has_fulltext`. The standalone `/text` endpoint was removed.

---

## 2. List / query mechanics (apply to all entity list endpoints)

Shared list params: `filter`, `search`, `sort`, `group_by`, `page`, `per_page` (alias
`per-page`), `cursor`, `sample`, `seed`, `select`, `api_key`.

### Filtering — `filter=`
- Syntax: `filter=attr:value,attr2:value2`.
- **AND** across attributes: comma. `filter=cited_by_count:>1,is_oa:true`
- **AND on same attribute**: repeat attr, or join values with `+`.
  `filter=institutions.country_code:fr+gb` (not valid for search/boolean/numeric filters)
- **OR** within one attribute: pipe `|`. Max **100** values per list. `country_code:fr|gb`
- **NOT**: prefix value with `!`. `country_code:!us`
- **Range / inequality**: `>` and `<`. `works_count:>1000`. Date ranges via `from_*`/`to_*`
  convenience filters (e.g. `from_publication_date`, `to_publication_date`).

### Searching
- Params (one per request): `search` (full-text, stemmed, stop-words removed, whole-word),
  `search.exact` (literal/unstemmed), `search.semantic` (AI embedding match).
- Per-field: `filter=<field>.search:<value>` (e.g. `title.search`, `abstract.search`,
  `fulltext.search`, `display_name.search`, `raw_author_name.search`). Append `.no_stem` on
  title/abstract to disable stemming.
- Query operators inside `search`: `AND`, `OR`, `NOT`, quoted phrases `"..."`, proximity
  `"a b"~5`, wildcard `machin*`, fuzzy `machin~1`.
- Results sorted by `relevance_score` desc when a search is active.
- Default search fields: Works = title+abstract+fulltext; Authors = display_name(+alts);
  Sources = display_name+alternate_titles+abbreviated_title; Institutions =
  display_name+alternatives+acronyms; Topics/Keywords = display_name+description.

### Sorting — `sort=`
- `sort=field` (asc) or `sort=field:desc`. Multiple comma-separated. `relevance_score`
  requires an active search or it errors.

### Pagination
- **Basic**: `page` (1–500, default 1), `per_page` (1–100, default 25). Hard cap:
  `page * per_page` ≤ **10,000**.
- **Cursor** (for > 10k): start `cursor=*`, then feed `meta.next_cursor` each request until
  `next_cursor` is null / `results` empty.

### Selecting — `select=`
- Comma-separated **top-level** fields only (nested like `open_access.is_oa` errors).
- Works on list + single-entity; NOT with `group_by` or autocomplete.

### Sampling — `sample=N`
- Max **10,000**. Add `seed=<int>` for reproducibility. Cannot combine with `sort` or `page`.

### Grouping — `group_by=`
- `group_by=<field>` or `group_by=<field>:include_unknown`. Combine with `filter`. Group
  paging is cursor-only, max **200 groups/page**.
- Group object: `{ "key": <value/id>, "key_display_name": <string>, "count": <int> }`.
  Unknown sentinels: numeric `-111`, string `"unknown"`, boolean → `false` bucket.

### Autocomplete
- `/autocomplete?q=<query>` (entity-type auto-detected) and `/autocomplete/<entity_type>?q=`.
  Entity types: works, authors, sources, institutions, concepts, publishers, funders.
- Params: `q` (required), `filter`, `search`. Result item:
  `{ id, display_name, hint, cited_by_count, entity_type, external_id, works_count }`.

### N-grams
- `/works/{id}/ngrams` → `{ meta:{count,doi,ngram_count}, ngrams:[{ngram, ngram_count,
  ngram_tokens, term_frequency}] }`. **Reported not-in-service as of late 2024; treat as
  unavailable.** Field names high-confidence but unverified.

---

## 3. Works

Endpoints: `GET /works`, `GET /works/{id}`, `GET /works/random`, `GET /autocomplete/works?q=`.

### Work object — top-level fields
| Field | Type | Meaning |
|---|---|---|
| `id` | string | OpenAlex ID URL |
| `doi` | string | DOI (canonical external ID) |
| `title` / `display_name` | string | Title (same value) |
| `publication_year` | integer | Year |
| `publication_date` | string | ISO `yyyy-mm-dd` |
| `ids` | object | `{openalex, doi, mag, pmid, pmcid}` |
| `language` | string | ISO 639-1 |
| `primary_location` | Location | Primary host location |
| `locations` | Location[] | All locations |
| `locations_count` | integer | Count of locations |
| `best_oa_location` | Location | Best OA location |
| `type` | string | Work type (e.g. `article`) |
| `type_crossref` | string | Crossref type |
| `indexed_in` | string[] | e.g. crossref, pubmed, doaj, arxiv |
| `open_access` | object | `{is_oa, oa_status, oa_url, any_repository_has_fulltext}` |
| `authorships` | Authorship[] | Authors + institutions |
| `countries_distinct_count` | integer | Distinct author countries |
| `institutions_distinct_count` | integer | Distinct institutions |
| `corresponding_author_ids` | string[] | Corresponding author OpenAlex IDs |
| `corresponding_institution_ids` | string[] | Corresponding institution IDs |
| `apc_list` / `apc_paid` | APC | List/paid article processing charge |
| `fwci` | float | Field-Weighted Citation Impact |
| `has_fulltext` | boolean | Fulltext indexed |
| `fulltext_origin` | string | `pdf` or `ngrams` |
| `cited_by_count` | integer | Citations received |
| `citation_normalized_percentile` | object | `{value, is_in_top_1_percent, is_in_top_10_percent}` |
| `cited_by_percentile_year` | object | `{min, max}` |
| `biblio` | object | `{volume, issue, first_page, last_page}` |
| `is_retracted` | boolean | Retracted |
| `is_paratext` | boolean | Paratext (cover, ToC, etc.) |
| `primary_topic` | Topic | Top topic |
| `topics` | Topic[] | Up to 3 topics |
| `keywords` | Keyword[] | `{id, display_name, score}` |
| `concepts` | Concept[] | Dehydrated concepts (legacy) |
| `mesh` | Mesh[] | MeSH tags (PubMed) |
| `sustainable_development_goals` | SDG[] | `{id, display_name, score}` |
| `grants` | Grant[] | `{funder, funder_display_name, award_id}` |
| `datasets` | string[] | Associated datasets |
| `versions` | string[] | OpenAlex IDs of other versions |
| `referenced_works` | string[] | Works this cites |
| `referenced_works_count` | integer | Count of references |
| `related_works` | string[] | Related work IDs |
| `abstract_inverted_index` | object | `{word: [positions]}` — reconstruct to get abstract |
| `counts_by_year` | {year, cited_by_count}[] | Citations/year (~10 yrs) |
| `cited_by_api_url` | string | API URL of citing works |
| `ngrams_url` | string | API URL for n-grams |
| `created_date` / `updated_date` | string | ISO dates |

### Nested objects
- **Authorship**: `author_position` (first|middle|last), `author` `{id, display_name, orcid}`,
  `institutions` `{id, display_name, ror, country_code, type, lineage[]}`, `affiliations`
  `{raw_affiliation_string, institution_ids[]}`, `countries[]`, `is_corresponding`,
  `raw_author_name`, `raw_affiliation_strings[]`.
- **Location**: `is_accepted`, `is_oa`, `is_published`, `landing_page_url`, `pdf_url`,
  `license`, `version` (publishedVersion|acceptedVersion|submittedVersion), `source`
  (DehydratedSource `{id, display_name, issn_l, issn[], host_organization,
  host_organization_name, host_organization_lineage[], is_in_doaj, is_oa, type}`).
- **APC**: `{value, currency, provenance (doaj|openapc), value_usd}`.
- **Topic**: `{id, display_name, score, domain{id,display_name}, field{...}, subfield{...}}`.
- **Mesh**: `{descriptor_ui, descriptor_name, qualifier_ui, qualifier_name, is_major_topic}`.

### Works filters (operators: `,`=AND `|`=OR `!`=NOT `>`/`<`=range)
**Attribute**: `authorships.author.id` (alias `author.id`), `authorships.author.orcid`
(alias `author.orcid`), `authorships.countries`, `authorships.affiliations.institution_ids`,
`authorships.institutions.id` (alias `institutions.id`), `authorships.institutions.ror`
(alias `institutions.ror`), `authorships.institutions.country_code`
(alias `institutions.country_code`), `authorships.institutions.type`,
`authorships.institutions.lineage`, `authorships.is_corresponding` (alias `is_corresponding`),
`apc_list.value`/`.currency`/`.provenance`/`.value_usd`,
`apc_paid.value`/`.currency`/`.provenance`/`.value_usd`,
`best_oa_location.is_accepted`/`.is_published`/`.license`/`.version`/`.source.id`/
`.source.issn`/`.source.is_in_doaj`/`.source.host_organization`/`.source.type`,
`biblio.volume`/`.issue`/`.first_page`/`.last_page`, `cited_by_count`, `concepts.id`
(alias `concept.id`), `concepts.wikidata`, `corresponding_author_ids`,
`corresponding_institution_ids`, `countries_distinct_count`, `doi`, `fulltext_origin`,
`fwci`, `grants.funder`, `grants.award_id`, `has_fulltext`, `ids.openalex` (alias `openalex`),
`ids.pmid` (alias `pmid`), `ids.pmcid`, `ids.mag` (alias `mag`), `indexed_in`,
`institutions_distinct_count`, `is_paratext`, `is_retracted`, `keywords.keyword`,
`keywords.id`, `language`, `locations.is_accepted`/`.is_oa`/`.is_published`/`.license`/
`.version`/`.source.id`/`.source.issn`/`.source.is_core`/`.source.is_in_doaj`/
`.source.host_organization`/`.source.type`, `locations_count`, `open_access.is_oa`
(alias `is_oa`), `open_access.oa_status` (alias `oa_status`),
`open_access.any_repository_has_fulltext`, `primary_location.*` (same sub-keys as locations),
`primary_topic.id`/`.domain.id`/`.field.id`/`.subfield.id`, `publication_year`,
`publication_date`, `sustainable_development_goals.id`,
`topics.id`/`.domain.id`/`.field.id`/`.subfield.id`, `type`, `type_crossref`.
**Convenience**: `abstract.search`, `authors_count`, `authorships.institutions.continent`
(alias `institutions.continent`), `authorships.institutions.is_global_south`
(alias `institutions.is_global_south`), `best_open_version`
(any|acceptedOrPublished|published), `cited_by` (ID), `cites` (ID), `concepts_count`,
`default.search`, `display_name.search`/`title.search`, `from_created_date`,
`from_publication_date`, `from_updated_date`, `fulltext.search`, `has_abstract`, `has_doi`,
`has_oa_accepted_or_published_version`, `has_oa_submitted_version`, `has_orcid`, `has_pmcid`,
`has_pmid`, `has_ngrams` (deprecated), `has_references`, `journal` (ID),
`locations.source.host_institution_lineage`, `locations.source.publisher_lineage`,
`mag_only`, `primary_location.source.has_issn`, `primary_location.source.publisher_lineage`,
`raw_affiliation_strings.search`, `related_to` (ID), `repository` (ID),
`title_and_abstract.search`, `to_created_date`, `to_publication_date`, `to_updated_date`,
`version`.

### Works examples
```
/works/W2741809807
/works/doi:10.7717/peerj.4375
/works/W2741809807?select=id,display_name,cited_by_count
/works?search=dna
/works?filter=title.search:cubist
/works?filter=publication_year:2020,is_oa:true
/works?filter=cited_by_count:>100,authorships.institutions.country_code:us
/works?filter=concepts.id:C71924100|C185592680
/works?filter=cites:W2741809807                # works citing a given work
/works?sample=20&seed=42&per-page=50
/works?group_by=open_access.oa_status
```

---

## 4. Authors

Endpoints: `GET /authors`, `GET /authors/{id}`, `GET /authors/random`,
`GET /autocomplete/authors?q=`. Accepted IDs: `A...`, ORCID URL, `orcid:...`.

### Author object
`id`, `orcid` (canonical), `display_name`, `display_name_alternatives[]`, `works_count`,
`cited_by_count`, `summary_stats` `{2yr_mean_citedness, h_index, i10_index}`, `affiliations[]`
`{institution(DehydratedInstitution), years[]}`, `last_known_institutions[]`, `topics[]`,
`topic_share[]`, `x_concepts[]` (deprecated), `counts_by_year[]`
`{year, works_count, cited_by_count}`, `ids` `{openalex, orcid, scopus, twitter, wikipedia,
mag}`, `works_api_url`, `created_date`, `updated_date`. (Live-only/legacy, verify:
`longest_name`, `parsed_longest_name{first,middle,last,suffix,nickname}`,
`last_known_institution`.)

### Author filters
`cited_by_count`, `works_count`, `display_name`, `has_orcid`, `id`, `openalex`, `orcid`,
`scopus`, `from_created_date`, `to_created_date`, `to_updated_date`,
`affiliations.institution.id`/`.ror`/`.country_code`/`.type`/`.lineage`,
`last_known_institutions.id`/`.ror`/`.country_code`/`.continent`/`.is_global_south`/`.type`/
`.lineage`, `ids.openalex`, `concepts.id`/`concept.id` (deprecated), `x_concepts.id`
(deprecated), `topics.id`, `topic_share.id`,
`summary_stats.2yr_mean_citedness`/`.h_index`/`.i10_index`. Sortable: `works_count`,
`cited_by_count`, `summary_stats.h_index`, `summary_stats.i10_index`, `display_name`.

```
/authors/A5023888391
/authors/https://orcid.org/0000-0001-6187-6610
/authors?filter=has_orcid:true,works_count:>100
/authors?filter=last_known_institutions.id:I27837315
```

---

## 5. Sources

Endpoints: `GET /sources` (alias `/journals`), `GET /sources/{id}`,
`GET /autocomplete/sources?q=`. Accepted IDs: `S...`, `issn:...`, MAG, Wikidata, Fatcat.

### Source object
`id`, `issn_l` (canonical), `issn[]`, `display_name`, `abbreviated_title`,
`alternate_titles[]`, `host_organization`, `host_organization_name`,
`host_organization_lineage[]`, `type` (journal|repository|conference|ebook platform|book
series|metadata|other), `is_oa`, `is_in_doaj`, `is_core`, `apc_prices[]` `{price, currency}`,
`apc_usd`, `country_code`, `homepage_url`, `societies[]` `{url, organization}`,
`summary_stats`, `counts_by_year[]`, `works_count`, `cited_by_count`, `ids` `{openalex,
issn_l, issn[], mag, wikidata, fatcat}`, `x_concepts[]` (deprecated), `works_api_url`,
`created_date`, `updated_date`.

### Source filters
`apc_prices.currency`, `apc_prices.price`, `apc_usd`, `cited_by_count`, `country_code`,
`host_organization` (alias `.id`), `host_organization_lineage`, `ids.openalex` (alias
`openalex`), `is_core`, `is_in_doaj`, `is_oa`, `issn`, `publisher`,
`summary_stats.2yr_mean_citedness`/`.h_index`/`.i10_index`, `type`, `works_count`,
`x_concepts.id` (deprecated). Convenience: `continent`, `default.search`,
`display_name.search`, `has_issn`, `is_global_south`.

```
/sources/S137773608
/sources/issn:2041-1723
/sources?filter=is_oa:true,has_issn:true
```

---

## 6. Institutions

Endpoints: `GET /institutions`, `GET /institutions/{id}`,
`GET /autocomplete/institutions?q=`. Accepted IDs: `I...`, `ror:<ror-url>`, MAG, Wikidata.

### Institution object
`id`, `ror` (canonical), `display_name`, `display_name_acronyms[]`,
`display_name_alternatives[]`, `country_code`, `type`
(Education|Healthcare|Company|Archive|Nonprofit|Government|Facility|Other), `homepage_url`,
`image_url`, `image_thumbnail_url`, `is_super_system`, `works_count`, `cited_by_count`,
`works_api_url`, `lineage[]`, `summary_stats`, `ids` `{openalex, ror, grid, mag, wikipedia,
wikidata}`, `geo` `{city, geonames_city_id, region, country_code, country, latitude,
longitude}`, `international` `{display_name: {lang: name}}`, `associated_institutions[]`
`{id, ror, display_name, country_code, type, relationship(parent|child|related)}`,
`repositories[]` `{id, display_name, host_organization, host_organization_name,
host_organization_lineage[]}`, `roles[]` `{role(institution|funder|publisher), id,
works_count}`, `counts_by_year[]`, `x_concepts[]` (deprecated), `created_date`,
`updated_date`. (Note: `topics`/`topic_share` are NOT institution fields despite the
developers-site summary listing them.)

### Institution filters
`cited_by_count`, `country_code`, `is_super_system`, `lineage` (institution ID), `openalex`,
`repositories.host_organization`/`.host_organization_lineage`/`.id`, `ror`,
`summary_stats.2yr_mean_citedness`/`.h_index`/`.i10_index`, `type`, `works_count`,
`x_concepts.id` (deprecated). Convenience: `continent` (e.g. `south_america`),
`is_global_south`, `has_ror`, `default.search`, `display_name.search`.

```
/institutions/I27837315
/institutions/ror:https://ror.org/00cvxb145
/institutions?filter=country_code:ca
/institutions?filter=continent:south_america
```

---

## 7. Topics & Keywords

### Topics — `GET /topics`, `GET /topics/{id}` (ID `T...`)
Object: `id`, `display_name`, `description`, `keywords[]`, `ids` `{openalex, wikipedia}`,
`subfield` `{id, display_name}`, `field` `{id, display_name}`, `domain` `{id, display_name}`,
`siblings[]` `{id, display_name}`, `works_count`, `cited_by_count`, `works_api_url`,
`created_date`, `updated_date`.
Filters: `cited_by_count`, `display_name`, `domain.id`, `field.id`, `subfield.id`, `id`,
`ids.openalex`, `openalex`, `works_count`, `from_created_date`. group_by: `domain.id`,
`field.id`, `subfield.id`.
```
/topics/T12419
/topics?filter=field.id:17,works_count:>1000
```

### Keywords — `GET /keywords`, `GET /keywords/{id}` (ID is a **slug**, e.g. `computer-science`)
Object: `id`, `display_name`, `works_count`, `cited_by_count`, `works_api_url`,
`created_date`, `updated_date`. Filters: `cited_by_count`, `display_name`, `id`,
`works_count`, `from_created_date`. Filter works by keyword: `keywords.id:computer-science`.
```
/keywords/computer-science
```

---

## 8. Publishers & Funders

### Publishers — `GET /publishers`, `GET /publishers/{id}` (ID `P...`)
Object: `id`, `display_name`, `alternate_titles[]`, `hierarchy_level` (0 = no parent),
`parent_publisher` `{id, display_name}`, `lineage[]`, `country_codes[]`, `homepage_url`,
`image_url`, `image_thumbnail_url`, `works_count`, `cited_by_count`, `summary_stats`, `ids`
`{openalex, ror, wikidata}`, `counts_by_year[]`, `roles[]`, `sources_api_url`,
`created_date`, `updated_date`.
Filters: `cited_by_count`, `continent`, `country_codes`, `display_name`, `hierarchy_level`,
`lineage`, `parent_publisher`, `roles.id`, `ids.openalex`/`ids.ror`/`ids.wikidata`,
`summary_stats.*`, `works_count`, `from_created_date`. group_by: `country_codes`,
`hierarchy_level`.
```
/publishers/P4310320990
/publishers?filter=country_codes:US,hierarchy_level:0
```

### Funders — `GET /funders`, `GET /funders/{id}` (ID `F...`)
Object: `id`, `display_name`, `alternate_titles[]`, `country_code`, `description`,
`homepage_url`, `image_url`, `image_thumbnail_url`, `grants_count` (**live returns
`awards_count`**), `works_count`, `cited_by_count`, `summary_stats`, `ids` `{openalex, ror,
wikidata, crossref, doi}`, `counts_by_year[]` (incl. live `oa_works_count`), `works_api_url`,
`created_date`, `updated_date`.
Filters: `awards_count`, `cited_by_count`, `continent`, `country_code`, `display_name`,
`is_global_south`, `roles.id`, `ids.*` (openalex/ror/wikidata/crossref/doi),
`openalex`/`ror`/`wikidata` aliases, `summary_stats.*`, `works_count`, `from_created_date`.
group_by: `country_code`, `continent`, `is_global_south`.
```
/funders/F4320321001
/funders?filter=country_code:US,is_global_south:true
```

---

## 9. Concepts (DEPRECATED) & Geo

### Concepts — `GET /concepts`, `GET /concepts/{id}` (ID `C...`)
**Deprecated**: "Concepts are deprecated and have been replaced by Topics. The concepts
endpoints remain functional but will not receive updates." Use `/topics`.
Object: `id`, `wikidata`, `display_name`, `level` (0 = top), `description`, `ids`
`{openalex, wikidata, mag, wikipedia, umls_aui, umls_cui}`, `image_url`,
`image_thumbnail_url`, `international`, `works_count`, `cited_by_count`, `summary_stats`,
`ancestors[]`, `related_concepts[]`, `counts_by_year[]`, `works_api_url`, `created_date`,
`updated_date`.
Filters: `ancestors.id`, `cited_by_count`, `display_name`, `has_wikidata`, `level`,
`ids.openalex`/`openalex_id`, `wikidata_id`, `summary_stats.*`, `works_count`,
`from_created_date`.

### Geo: Countries — `GET /countries`, `GET /countries/{CC}` (ISO-2)
Object: `id`, `display_name`, `country_code`, `description`, `display_name_alternatives[]`,
`ids` `{openalex, iso, wikidata, wikipedia}`, `continent` `{id, display_name}`,
`is_global_south`, `works_count`, `cited_by_count`, `works_api_url`, `authors_api_url`,
`institutions_api_url`, `created_date`, `updated_date`.

### Geo: Continents — `GET /continents`, `GET /continents/{Q}` (Wikidata Q-ID)
Object: `id`, `display_name`, `description`, `display_name_alternatives[]`, `ids`
`{openalex, wikidata}`, `countries[]` `{id, display_name}`, `works_count`, `cited_by_count`,
`created_date`, `updated_date`.

### Supporting taxonomy endpoints
`/domains`, `/fields`, `/subfields` (Topic hierarchy levels); `/sdgs`, `/languages`,
`/work-types`, `/source-types`, `/institution-types`, `/licenses`; `/autocomplete/{entity}`;
`/rate-limit`.
