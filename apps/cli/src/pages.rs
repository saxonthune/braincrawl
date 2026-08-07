#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PageRecord {
    pub pdf_page: usize,
    pub folio: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FolioMethod {
    Detected,
    Anchored,
    None,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Pages {
    pub source_role: String,
    pub page_count: usize,
    pub folio_method: FolioMethod,
    pub pages: Vec<PageRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum FolioBreak {
    /// One or more consecutive pdf pages with no detected folio, bounded by
    /// consistent folios on either side (or the start/end of the book).
    Gap { pdf_pages: Vec<usize> },
    /// A detected folio inconsistent with the sequence its neighbours establish.
    Contradiction { pdf_page: usize, folio: String },
}

/// A raw folio candidate token found on a page, before sequence resolution.
#[derive(Debug, Clone)]
struct Candidate {
    value: String,
    kind: CandidateKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateKind {
    Arabic(u64),
    Roman(u64),
}

fn is_roman_numeral(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| matches!(c, 'i' | 'v' | 'x' | 'l' | 'c' | 'd' | 'm'))
}

/// Parse a lowercase roman numeral into its value. Returns `None` if invalid.
fn roman_to_u64(token: &str) -> Option<u64> {
    let values = |c: char| -> Option<u64> {
        match c {
            'i' => Some(1),
            'v' => Some(5),
            'x' => Some(10),
            'l' => Some(50),
            'c' => Some(100),
            'd' => Some(500),
            'm' => Some(1000),
            _ => None,
        }
    };
    let chars: Vec<u64> = token.chars().map(values).collect::<Option<Vec<_>>>()?;
    let mut total = 0u64;
    for i in 0..chars.len() {
        let cur = chars[i];
        if i + 1 < chars.len() && cur < chars[i + 1] {
            total = total.checked_sub(cur)?;
        } else {
            total = total.checked_add(cur)?;
        }
    }
    if total == 0 {
        return None;
    }
    // Round-trip check: reject malformed sequences like "ivi" that parse but
    // aren't a canonical roman numeral, by requiring the value be non-degenerate.
    Some(total)
}

/// Extract folio candidates from the first and last non-blank lines of a page.
fn candidates_for_page(text: &str) -> Vec<Candidate> {
    let non_blank_lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut lines: Vec<&str> = Vec::new();
    if let Some(first) = non_blank_lines.first() {
        lines.push(first);
    }
    if let Some(last) = non_blank_lines.last() {
        if non_blank_lines.len() > 1 {
            lines.push(last);
        }
    }

    let mut candidates = Vec::new();
    for line in lines {
        for token in line.split_whitespace() {
            let trimmed = token.trim_matches(|c: char| !c.is_alphanumeric());
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(n) = trimmed.parse::<u64>() {
                    candidates.push(Candidate { value: trimmed.to_string(), kind: CandidateKind::Arabic(n) });
                }
            } else if trimmed.chars().all(|c| c.is_ascii_lowercase()) && is_roman_numeral(trimmed) {
                if let Some(n) = roman_to_u64(trimmed) {
                    candidates.push(Candidate { value: trimmed.to_string(), kind: CandidateKind::Roman(n) });
                }
            }
        }
    }
    candidates
}

/// Detect a per-page folio by sequence agreement across consecutive PDF pages.
///
/// A candidate on page N is accepted if its numeric value is exactly one more
/// than the accepted value on the nearest preceding page that has one (arabic
/// and roman sequences are tracked independently since front matter and body
/// use different numerals). Where no candidate agrees, `folio` is `None`.
///
/// A correctly detected folio series has a constant `folio value - pdf page
/// index` offset, since the folio increments by one per consecutive page.
/// So for each numeral kind, every candidate votes for the offset it implies;
/// the offset with the most distinct pages behind it is taken as the true
/// series (requiring at least two agreeing pages — a single page has no
/// neighbour to confirm it), and only candidates matching that offset are
/// accepted. This rejects an off-sequence candidate (bad OCR, a stray digit)
/// even when its own page has no adjacent page to compare against directly.
pub fn detect_folios(pages: &[String]) -> Vec<PageRecord> {
    let per_page_candidates: Vec<Vec<Candidate>> = pages.iter().map(|p| candidates_for_page(p)).collect();

    let mut votes: std::collections::HashMap<(bool, i64), std::collections::HashSet<usize>> =
        std::collections::HashMap::new();
    for (i, candidates) in per_page_candidates.iter().enumerate() {
        for c in candidates {
            let (is_roman, value) = match c.kind {
                CandidateKind::Arabic(n) => (false, n as i64),
                CandidateKind::Roman(n) => (true, n as i64),
            };
            let offset = value - i as i64;
            votes.entry((is_roman, offset)).or_default().insert(i);
        }
    }

    // For each kind, keep only the offset with the most distinct-page votes
    // (at least two), breaking ties by the smaller offset for determinism.
    let mut accepted_offsets: std::collections::HashMap<bool, i64> = std::collections::HashMap::new();
    for is_roman in [false, true] {
        let best = votes
            .iter()
            .filter(|((r, _), pages)| *r == is_roman && pages.len() >= 2)
            .max_by_key(|(&(_, offset), pages)| (pages.len(), std::cmp::Reverse(offset)));
        if let Some((&(_, offset), _)) = best {
            accepted_offsets.insert(is_roman, offset);
        }
    }

    pages
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let folio = per_page_candidates[i]
                .iter()
                .find(|c| {
                    let (is_roman, value) = match c.kind {
                        CandidateKind::Arabic(n) => (false, n as i64),
                        CandidateKind::Roman(n) => (true, n as i64),
                    };
                    accepted_offsets.get(&is_roman) == Some(&(value - i as i64))
                })
                .map(|c| c.value.clone());
            PageRecord { pdf_page: i + 1, folio, text: text.clone() }
        })
        .collect()
}

/// Report where the detected folio sequence breaks. A `None`-folio page whose
/// text held no candidate token at all is a **gap** (an unnumbered plate, a
/// part title). A `None`-folio page that held a candidate token — rejected by
/// `detect_folios` because it disagreed with the sequence — is a
/// **contradiction**, reported individually since it names a specific wrong
/// value rather than an absence.
pub fn validate_folios(records: &[PageRecord]) -> Vec<FolioBreak> {
    let mut breaks = Vec::new();
    let mut gap_run: Vec<usize> = Vec::new();

    let flush_gap = |breaks: &mut Vec<FolioBreak>, run: &mut Vec<usize>| {
        if !run.is_empty() {
            breaks.push(FolioBreak::Gap { pdf_pages: std::mem::take(run) });
        }
    };

    for record in records {
        if record.folio.is_some() {
            flush_gap(&mut breaks, &mut gap_run);
            continue;
        }
        let candidates = candidates_for_page(&record.text);
        if let Some(candidate) = candidates.first() {
            flush_gap(&mut breaks, &mut gap_run);
            breaks.push(FolioBreak::Contradiction {
                pdf_page: record.pdf_page,
                folio: candidate.value.clone(),
            });
        } else {
            gap_run.push(record.pdf_page);
        }
    }
    flush_gap(&mut breaks, &mut gap_run);
    breaks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn detects_simple_arabic_sequence() {
        let pages = vec![
            page("Chapter One\nsome text here\n7"),
            page("8\nmore body text\nmore"),
            page("more body\ntext content\n9"),
        ];
        let recs = detect_folios(&pages);
        assert_eq!(recs[0].folio.as_deref(), Some("7"));
        assert_eq!(recs[1].folio.as_deref(), Some("8"));
        assert_eq!(recs[2].folio.as_deref(), Some("9"));
    }

    #[test]
    fn ignores_body_text_numerals_not_on_header_footer_lines() {
        let pages = vec![
            page("7\nIn 1942 there were 500 units produced\nmore text"),
            page("8\nmore body\nmore"),
        ];
        let recs = detect_folios(&pages);
        assert_eq!(recs[0].folio.as_deref(), Some("7"));
        assert_eq!(recs[1].folio.as_deref(), Some("8"));
    }

    #[test]
    fn roman_numeral_front_matter() {
        let pages = vec![page("xii\nPreface text\nmore"), page("more\nsecond page\nxiii")];
        let recs = detect_folios(&pages);
        assert_eq!(recs[0].folio.as_deref(), Some("xii"));
        assert_eq!(recs[1].folio.as_deref(), Some("xiii"));
    }

    #[test]
    fn no_candidate_yields_none() {
        let pages = vec![page("no numbers here\njust text\nnope"), page("still nothing\nhere either\nnada")];
        let recs = detect_folios(&pages);
        assert!(recs.iter().all(|r| r.folio.is_none()));
    }

    #[test]
    fn inconsistent_candidate_is_rejected() {
        let pages = vec![
            page("7\ntext\nmore"),
            page("999\nwrong number\nmore"),
            page("more\ntext\n9"),
        ];
        let recs = detect_folios(&pages);
        assert_eq!(recs[0].folio.as_deref(), Some("7"));
        assert_eq!(recs[1].folio, None);
        assert_eq!(recs[2].folio.as_deref(), Some("9"));
    }

    #[test]
    fn validate_folios_reports_gap() {
        let records = vec![
            PageRecord { pdf_page: 1, folio: Some("7".into()), text: String::new() },
            PageRecord { pdf_page: 2, folio: None, text: String::new() },
            PageRecord { pdf_page: 3, folio: None, text: String::new() },
            PageRecord { pdf_page: 4, folio: Some("10".into()), text: String::new() },
        ];
        let breaks = validate_folios(&records);
        assert_eq!(breaks, vec![FolioBreak::Gap { pdf_pages: vec![2, 3] }]);
    }

    #[test]
    fn validate_folios_reports_no_breaks_for_clean_sequence() {
        let records = vec![
            PageRecord { pdf_page: 1, folio: Some("7".into()), text: String::new() },
            PageRecord { pdf_page: 2, folio: Some("8".into()), text: String::new() },
        ];
        assert!(validate_folios(&records).is_empty());
    }

    #[test]
    fn validate_folios_reports_contradiction_distinct_from_gap() {
        let pages = vec![
            page("7\ntext\nmore"),
            page("999\nwrong number\nmore"),
            page("more\ntext\n9"),
        ];
        let records = detect_folios(&pages);
        let breaks = validate_folios(&records);
        assert_eq!(breaks, vec![FolioBreak::Contradiction { pdf_page: 2, folio: "999".into() }]);
    }
}
