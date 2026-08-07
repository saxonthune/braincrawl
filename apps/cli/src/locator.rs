use crate::outline::Outline;
use crate::pages::Pages;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRange {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    /// A printed folio range, e.g. p.9-12. Matches `PageRecord::folio` exactly.
    Printed(PageRange),
    /// A raw PDF page range, e.g. pdf 16-19. Matches `PageRecord::pdf_page` exactly.
    Pdf(PageRange),
    /// An outline section id, optionally trimmed to its first/last N pages.
    Section { id: String, head: Option<usize>, tail: Option<usize> },
    /// A page-text search; resolves to every page containing the needle.
    Quote(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSpan {
    pub pdf_pages: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocatorError {
    FolioNotFound { folio: String, pages_lacking_folio: usize },
    PdfPageOutOfRange { requested: usize, page_count: usize },
    NoOutline,
    SectionNotFound { id: String, known: Vec<String> },
    QuoteNotFound { needle: String },
    PartialRange { found: Vec<usize>, missing: Vec<String> },
}

impl std::fmt::Display for LocatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocatorError::FolioNotFound { folio, pages_lacking_folio } => write!(
                f,
                "no page has folio '{folio}' ({pages_lacking_folio} page(s) have no detected folio)"
            ),
            LocatorError::PdfPageOutOfRange { requested, page_count } => {
                write!(f, "pdf page {requested} is out of range (page_count {page_count})")
            }
            LocatorError::NoOutline => write!(f, "no outline artifact provided; --section requires one"),
            LocatorError::SectionNotFound { id, known } => {
                write!(f, "unknown section id '{id}'; known sections: {}", known.join(", "))
            }
            LocatorError::QuoteNotFound { needle } => write!(f, "no page contains: {needle}"),
            LocatorError::PartialRange { found, missing } => write!(
                f,
                "found pdf page(s) {found:?}; missing: {}",
                missing.join(", ")
            ),
        }
    }
}

impl std::error::Error for LocatorError {}

/// Resolve a `Locator` against already-loaded `Pages`/`Outline` artifacts.
/// Pure: no store access, no IO.
pub fn resolve(loc: &Locator, pages: &Pages, outline: Option<&Outline>) -> Result<PageSpan, LocatorError> {
    match loc {
        Locator::Printed(range) => resolve_printed(range, pages),
        Locator::Pdf(range) => resolve_pdf(range, pages),
        Locator::Section { id, head, tail } => resolve_section(id, *head, *tail, pages, outline),
        Locator::Quote(needle) => resolve_quote(needle, pages),
    }
}

fn folio_page(pages: &Pages, folio: &str) -> Option<usize> {
    pages.pages.iter().find(|p| p.folio.as_deref() == Some(folio)).map(|p| p.pdf_page)
}

fn resolve_printed(range: &PageRange, pages: &Pages) -> Result<PageSpan, LocatorError> {
    let lacking = pages.pages.iter().filter(|p| p.folio.is_none()).count();
    let start = folio_page(pages, &range.start).ok_or_else(|| LocatorError::FolioNotFound {
        folio: range.start.clone(),
        pages_lacking_folio: lacking,
    })?;
    let end = folio_page(pages, &range.end).ok_or_else(|| LocatorError::FolioNotFound {
        folio: range.end.clone(),
        pages_lacking_folio: lacking,
    })?;
    let (lo, hi) = if start <= end { (start, end) } else { (end, start) };
    Ok(PageSpan { pdf_pages: (lo..=hi).collect() })
}

fn resolve_pdf(range: &PageRange, pages: &Pages) -> Result<PageSpan, LocatorError> {
    let start: usize = range
        .start
        .parse()
        .map_err(|_| LocatorError::PdfPageOutOfRange { requested: 0, page_count: pages.page_count })?;
    let end: usize = range
        .end
        .parse()
        .map_err(|_| LocatorError::PdfPageOutOfRange { requested: 0, page_count: pages.page_count })?;
    for &p in &[start, end] {
        if p == 0 || p > pages.page_count {
            return Err(LocatorError::PdfPageOutOfRange { requested: p, page_count: pages.page_count });
        }
    }
    let (lo, hi) = if start <= end { (start, end) } else { (end, start) };
    Ok(PageSpan { pdf_pages: (lo..=hi).collect() })
}

fn resolve_section(
    id: &str,
    head: Option<usize>,
    tail: Option<usize>,
    pages: &Pages,
    outline: Option<&Outline>,
) -> Result<PageSpan, LocatorError> {
    let outline = outline.ok_or(LocatorError::NoOutline)?;
    let section = outline.sections.iter().find(|s| s.id == id).ok_or_else(|| LocatorError::SectionNotFound {
        id: id.to_string(),
        known: outline.sections.iter().map(|s| s.id.clone()).collect(),
    })?;

    let full: Vec<usize> = (section.start_pdf_page..=section.end_pdf_page).collect();
    let trimmed = match (head, tail) {
        (Some(n), _) => full.into_iter().take(n).collect(),
        (_, Some(n)) => {
            let len = full.len();
            full.into_iter().skip(len.saturating_sub(n)).collect()
        }
        (None, None) => full,
    };

    let found: Vec<usize> = trimmed.iter().copied().filter(|&p| p <= pages.page_count).collect();
    let missing: Vec<String> = trimmed.iter().filter(|&&p| p > pages.page_count).map(|p| p.to_string()).collect();
    if !missing.is_empty() {
        return Err(LocatorError::PartialRange { found, missing });
    }
    Ok(PageSpan { pdf_pages: found })
}

fn resolve_quote(needle: &str, pages: &Pages) -> Result<PageSpan, LocatorError> {
    let found: Vec<usize> = pages
        .pages
        .iter()
        .filter(|p| p.text.contains(needle))
        .map(|p| p.pdf_page)
        .collect();
    if found.is_empty() {
        return Err(LocatorError::QuoteNotFound { needle: needle.to_string() });
    }
    Ok(PageSpan { pdf_pages: found })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::Section;
    use crate::pages::{FolioMethod, PageRecord};

    fn make_pages() -> Pages {
        Pages {
            source_role: "text".into(),
            page_count: 5,
            folio_method: FolioMethod::Detected,
            pages: vec![
                PageRecord { pdf_page: 1, folio: None, text: "front matter".into() },
                PageRecord { pdf_page: 2, folio: Some("1".into()), text: "territorialization begins".into() },
                PageRecord { pdf_page: 3, folio: Some("2".into()), text: "more body text".into() },
                PageRecord { pdf_page: 4, folio: Some("3".into()), text: "final content".into() },
                PageRecord { pdf_page: 5, folio: Some("4".into()), text: "the end".into() },
            ],
        }
    }

    fn make_outline() -> Outline {
        Outline {
            source_role: "text".into(),
            sections: vec![Section {
                id: "ch01".into(),
                title: "Chapter One".into(),
                level: 1,
                start_pdf_page: 2,
                end_pdf_page: 5,
            }],
        }
    }

    #[test]
    fn printed_resolves_exactly() {
        let pages = make_pages();
        let loc = Locator::Printed(PageRange { start: "1".into(), end: "3".into() });
        let span = resolve(&loc, &pages, None).unwrap();
        assert_eq!(span.pdf_pages, vec![2, 3, 4]);
    }

    #[test]
    fn printed_missing_folio_reports_count() {
        let pages = make_pages();
        let loc = Locator::Printed(PageRange { start: "99".into(), end: "99".into() });
        let err = resolve(&loc, &pages, None).unwrap_err();
        assert_eq!(err, LocatorError::FolioNotFound { folio: "99".into(), pages_lacking_folio: 1 });
    }

    #[test]
    fn pdf_resolves_exactly() {
        let pages = make_pages();
        let loc = Locator::Pdf(PageRange { start: "2".into(), end: "4".into() });
        let span = resolve(&loc, &pages, None).unwrap();
        assert_eq!(span.pdf_pages, vec![2, 3, 4]);
    }

    #[test]
    fn pdf_out_of_range_errors() {
        let pages = make_pages();
        let loc = Locator::Pdf(PageRange { start: "1".into(), end: "50".into() });
        let err = resolve(&loc, &pages, None).unwrap_err();
        assert_eq!(err, LocatorError::PdfPageOutOfRange { requested: 50, page_count: 5 });
    }

    #[test]
    fn section_resolves_through_outline() {
        let pages = make_pages();
        let outline = make_outline();
        let loc = Locator::Section { id: "ch01".into(), head: None, tail: None };
        let span = resolve(&loc, &pages, Some(&outline)).unwrap();
        assert_eq!(span.pdf_pages, vec![2, 3, 4, 5]);
    }

    #[test]
    fn section_head_takes_first_n() {
        let pages = make_pages();
        let outline = make_outline();
        let loc = Locator::Section { id: "ch01".into(), head: Some(2), tail: None };
        let span = resolve(&loc, &pages, Some(&outline)).unwrap();
        assert_eq!(span.pdf_pages, vec![2, 3]);
    }

    #[test]
    fn section_tail_takes_last_n() {
        let pages = make_pages();
        let outline = make_outline();
        let loc = Locator::Section { id: "ch01".into(), head: None, tail: Some(2) };
        let span = resolve(&loc, &pages, Some(&outline)).unwrap();
        assert_eq!(span.pdf_pages, vec![4, 5]);
    }

    #[test]
    fn section_without_outline_errors() {
        let pages = make_pages();
        let loc = Locator::Section { id: "ch01".into(), head: None, tail: None };
        let err = resolve(&loc, &pages, None).unwrap_err();
        assert_eq!(err, LocatorError::NoOutline);
    }

    #[test]
    fn unknown_section_lists_known_ids() {
        let pages = make_pages();
        let outline = make_outline();
        let loc = Locator::Section { id: "chXX".into(), head: None, tail: None };
        let err = resolve(&loc, &pages, Some(&outline)).unwrap_err();
        assert_eq!(err, LocatorError::SectionNotFound { id: "chXX".into(), known: vec!["ch01".into()] });
    }

    #[test]
    fn quote_finds_containing_pages() {
        let pages = make_pages();
        let loc = Locator::Quote("territorialization".into());
        let span = resolve(&loc, &pages, None).unwrap();
        assert_eq!(span.pdf_pages, vec![2]);
    }

    #[test]
    fn quote_not_found_errors() {
        let pages = make_pages();
        let loc = Locator::Quote("nonexistent phrase".into());
        let err = resolve(&loc, &pages, None).unwrap_err();
        assert_eq!(err, LocatorError::QuoteNotFound { needle: "nonexistent phrase".into() });
    }
}
