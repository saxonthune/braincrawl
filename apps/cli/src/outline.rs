#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub level: usize,
    pub start_pdf_page: usize,
    pub end_pdf_page: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Outline {
    pub source_role: String,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutlineError {
    DuplicateId { id: String },
    InvalidRange { id: String, start: usize, end: usize },
    OverlappingRanges { a: String, b: String },
    OutOfBounds { id: String, end: usize, page_count: usize },
}

impl std::fmt::Display for OutlineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutlineError::DuplicateId { id } => write!(f, "duplicate section id: {id}"),
            OutlineError::InvalidRange { id, start, end } => {
                write!(f, "section {id}: start_pdf_page {start} > end_pdf_page {end}")
            }
            OutlineError::OverlappingRanges { a, b } => {
                write!(f, "sections {a} and {b} have overlapping page ranges")
            }
            OutlineError::OutOfBounds { id, end, page_count } => {
                write!(f, "section {id}: end_pdf_page {end} exceeds page_count {page_count}")
            }
        }
    }
}

impl std::error::Error for OutlineError {}

/// Validate an outline: ids unique, each range non-inverted, ranges within
/// `page_count`, and no two sections overlapping.
pub fn validate(outline: &Outline, page_count: usize) -> Result<(), Vec<OutlineError>> {
    let mut errors = Vec::new();

    let mut seen_ids = std::collections::HashSet::new();
    for section in &outline.sections {
        if !seen_ids.insert(section.id.clone()) {
            errors.push(OutlineError::DuplicateId { id: section.id.clone() });
        }
        if section.start_pdf_page > section.end_pdf_page {
            errors.push(OutlineError::InvalidRange {
                id: section.id.clone(),
                start: section.start_pdf_page,
                end: section.end_pdf_page,
            });
        }
        if section.end_pdf_page > page_count {
            errors.push(OutlineError::OutOfBounds {
                id: section.id.clone(),
                end: section.end_pdf_page,
                page_count,
            });
        }
    }

    let mut by_start: Vec<&Section> = outline.sections.iter().collect();
    by_start.sort_by_key(|s| s.start_pdf_page);
    for pair in by_start.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a.start_pdf_page <= a.end_pdf_page && b.start_pdf_page <= b.end_pdf_page && b.start_pdf_page <= a.end_pdf_page {
            errors.push(OutlineError::OverlappingRanges { a: a.id.clone(), b: b.id.clone() });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(id: &str, start: usize, end: usize) -> Section {
        Section { id: id.to_string(), title: id.to_string(), level: 1, start_pdf_page: start, end_pdf_page: end }
    }

    #[test]
    fn validates_clean_outline() {
        let outline = Outline {
            source_role: "text".into(),
            sections: vec![section("ch01", 16, 33), section("ch02", 34, 50)],
        };
        assert!(validate(&outline, 200).is_ok());
    }

    #[test]
    fn rejects_duplicate_ids() {
        let outline = Outline {
            source_role: "text".into(),
            sections: vec![section("ch01", 1, 5), section("ch01", 6, 10)],
        };
        let errors = validate(&outline, 200).unwrap_err();
        assert!(errors.contains(&OutlineError::DuplicateId { id: "ch01".into() }));
    }

    #[test]
    fn rejects_inverted_range() {
        let outline = Outline { source_role: "text".into(), sections: vec![section("ch01", 10, 5)] };
        let errors = validate(&outline, 200).unwrap_err();
        assert!(errors.contains(&OutlineError::InvalidRange { id: "ch01".into(), start: 10, end: 5 }));
    }

    #[test]
    fn rejects_out_of_bounds() {
        let outline = Outline { source_role: "text".into(), sections: vec![section("ch01", 1, 500)] };
        let errors = validate(&outline, 200).unwrap_err();
        assert!(errors.contains(&OutlineError::OutOfBounds { id: "ch01".into(), end: 500, page_count: 200 }));
    }

    #[test]
    fn rejects_overlapping_ranges() {
        let outline = Outline {
            source_role: "text".into(),
            sections: vec![section("ch01", 1, 20), section("ch02", 15, 30)],
        };
        let errors = validate(&outline, 200).unwrap_err();
        assert!(errors.contains(&OutlineError::OverlappingRanges { a: "ch01".into(), b: "ch02".into() }));
    }
}
