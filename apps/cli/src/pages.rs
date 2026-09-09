#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PageRecord {
    pub page_index: usize,
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

/// How a text artifact's page grid is defined — the structural analog of a
/// folio anchor. A PDF carries its own page grid; a text artifact does not, so
/// the operator declares the boundary rule that cuts the text into pages.
#[derive(Debug, Clone)]
pub enum PageSeparator {
    /// Split on the form-feed character (`\x0c`) — what `pdftotext` and most
    /// extractors write at each page boundary.
    FormFeed,
    /// Split wherever at least this many consecutive blank lines occur.
    BlankLines(usize),
    /// Start a new page at each line matching this pattern (a running header, a
    /// bare page number). The matching line becomes the first line of the page.
    Regex(regex::Regex),
}

impl std::str::FromStr for PageSeparator {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "form-feed" {
            return Ok(Self::FormFeed);
        }
        if let Some(n) = s.strip_prefix("blank-lines:") {
            let n: usize = n
                .parse()
                .map_err(|_| format!("separator '{s}': '{n}' is not a line count"))?;
            if n == 0 {
                return Err(format!("separator '{s}': blank-lines count must be at least 1"));
            }
            return Ok(Self::BlankLines(n));
        }
        if let Some(pat) = s.strip_prefix("regex:") {
            let re = regex::Regex::new(pat)
                .map_err(|e| format!("separator '{s}': invalid regex: {e}"))?;
            return Ok(Self::Regex(re));
        }
        Err(format!(
            "separator '{s}': expected 'form-feed', 'blank-lines:<n>', or 'regex:<pattern>'"
        ))
    }
}

impl PageSeparator {
    /// Cut `text` into pages by this rule. Empty text yields no pages.
    pub fn split(&self, text: &str) -> Vec<String> {
        match self {
            Self::FormFeed => text.split_terminator('\u{000C}').map(str::to_string).collect(),
            Self::BlankLines(n) => split_on_blank_lines(text, *n),
            Self::Regex(re) => split_at_matching_line(text, re),
        }
    }
}

fn split_on_blank_lines(text: &str, n: usize) -> Vec<String> {
    let mut pages = Vec::new();
    let mut current = String::new();
    let mut blank_run = 0usize;
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            blank_run += 1;
        } else {
            if blank_run >= n && !current.trim().is_empty() {
                pages.push(std::mem::take(&mut current));
            }
            blank_run = 0;
        }
        current.push_str(line);
    }
    if !current.trim().is_empty() {
        pages.push(current);
    }
    pages
}

fn split_at_matching_line(text: &str, re: &regex::Regex) -> Vec<String> {
    let mut pages = Vec::new();
    let mut current = String::new();
    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        if re.is_match(content) && !current.is_empty() {
            pages.push(std::mem::take(&mut current));
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        pages.push(current);
    }
    pages
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum FolioBreak {
    /// One or more consecutive pages with no detected folio, bounded by
    /// consistent folios on either side (or the start/end of the book).
    Gap { page_indices: Vec<usize> },
    /// A detected folio inconsistent with the sequence its neighbours establish.
    Contradiction { page_index: usize, folio: String },
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

/// An operator-supplied fact: the page at `page_index` bears printed folio `folio`.
///
/// Parsed from `--anchor <page_index>=<folio>`. Unlike a single global offset, a
/// list of anchors describes a piecewise mapping: front matter in roman
/// numerals, an unnumbered plate section, or a scan whose first leaf is folio 1
/// while its second is folio 3 all break a constant offset but are exactly
/// describable as a handful of anchors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolioAnchor {
    pub page_index: usize,
    pub folio: String,
}

impl std::str::FromStr for FolioAnchor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (page, folio) = s
            .split_once('=')
            .ok_or_else(|| format!("anchor '{s}' is not in <page_index>=<folio> form"))?;
        let page_index: usize = page
            .trim()
            .parse()
            .map_err(|_| format!("anchor '{s}': '{page}' is not a page index"))?;
        if page_index == 0 {
            return Err(format!("anchor '{s}': page indices are numbered from 1"));
        }
        let folio = folio.trim();
        if folio.is_empty() {
            return Err(format!("anchor '{s}': folio is empty"));
        }
        if folio_kind(folio).is_none() {
            return Err(format!(
                "anchor '{s}': folio '{folio}' is neither an arabic number nor a lowercase roman numeral"
            ));
        }
        Ok(FolioAnchor { page_index, folio: folio.to_string() })
    }
}

/// Classify a folio string so the next page's folio can be produced in the same
/// numeral system. `None` if the string is neither form.
fn folio_kind(folio: &str) -> Option<CandidateKind> {
    if !folio.is_empty() && folio.chars().all(|c| c.is_ascii_digit()) {
        return folio.parse::<u64>().ok().map(CandidateKind::Arabic);
    }
    if folio.chars().all(|c| c.is_ascii_lowercase()) && is_roman_numeral(folio) {
        return roman_to_u64(folio).map(CandidateKind::Roman);
    }
    None
}

/// Render a numeric value back into the numeral system it came from.
fn render_folio(kind: CandidateKind, value: u64) -> String {
    match kind {
        CandidateKind::Arabic(_) => value.to_string(),
        CandidateKind::Roman(_) => u64_to_roman(value),
    }
}

fn u64_to_roman(mut value: u64) -> String {
    const TABLE: [(u64, &str); 13] = [
        (1000, "m"), (900, "cm"), (500, "d"), (400, "cd"),
        (100, "c"), (90, "xc"), (50, "l"), (40, "xl"),
        (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i"),
    ];
    let mut out = String::new();
    for (n, sym) in TABLE {
        while value >= n {
            out.push_str(sym);
            value -= n;
        }
    }
    out
}

/// Assign folios from operator-supplied anchors rather than from the page text.
///
/// Anchors are sorted by page index and each governs the run of pages from itself
/// up to (not including) the next anchor, incrementing by one per page in its
/// own numeral system. Pages before the first anchor get no folio — the
/// operator has said nothing about them, and guessing backwards would reinstate
/// exactly the inference anchoring exists to replace.
pub fn anchor_folios(pages: &[String], anchors: &[FolioAnchor]) -> Vec<PageRecord> {
    let mut sorted: Vec<&FolioAnchor> = anchors.iter().collect();
    sorted.sort_by_key(|a| a.page_index);

    pages
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let page_index = i + 1;
            let governing = sorted.iter().rev().find(|a| a.page_index <= page_index);
            let folio = governing.and_then(|a| {
                let kind = folio_kind(&a.folio)?;
                let base = match kind {
                    CandidateKind::Arabic(n) | CandidateKind::Roman(n) => n,
                };
                let value = base + (page_index - a.page_index) as u64;
                Some(render_folio(kind, value))
            });
            PageRecord { page_index, folio, text: text.clone() }
        })
        .collect()
}

/// One stretch of pages over which the folio advances by one per page
/// without changing numeral system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolioRun {
    pub first_page_index: usize,
    pub last_page_index: usize,
    pub first_folio: String,
    pub last_folio: String,
}

impl std::fmt::Display for FolioRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.first_page_index == self.last_page_index {
            write!(f, "artifact-page {} = folio {}", self.first_page_index, self.first_folio)
        } else {
            write!(
                f,
                "artifact-page {}-{} = folio {}-{}",
                self.first_page_index, self.last_page_index, self.first_folio, self.last_folio
            )
        }
    }
}

/// Collapse a folio assignment into runs, so the operator can eyeball the whole
/// mapping as a few lines instead of one line per page. A run breaks wherever
/// the page index skips, the numeral system changes, or the folio fails to
/// advance by exactly one — which is precisely where a mapping needs checking.
pub fn folio_runs(records: &[PageRecord]) -> Vec<FolioRun> {
    let mut runs: Vec<FolioRun> = Vec::new();
    let mut prev: Option<(usize, CandidateKind, u64)> = None;

    for r in records {
        let Some(folio) = &r.folio else {
            prev = None;
            continue;
        };
        let Some(kind) = folio_kind(folio) else {
            prev = None;
            continue;
        };
        let value = match kind {
            CandidateKind::Arabic(n) | CandidateKind::Roman(n) => n,
        };
        let continues = matches!(prev, Some((page, prev_kind, prev_value))
            if page + 1 == r.page_index
                && std::mem::discriminant(&prev_kind) == std::mem::discriminant(&kind)
                && prev_value + 1 == value);

        if continues {
            let run = runs.last_mut().expect("a continuing run has a predecessor");
            run.last_page_index = r.page_index;
            run.last_folio = folio.clone();
        } else {
            runs.push(FolioRun {
                first_page_index: r.page_index,
                last_page_index: r.page_index,
                first_folio: folio.clone(),
                last_folio: folio.clone(),
            });
        }
        prev = Some((r.page_index, kind, value));
    }
    runs
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

/// Detect a per-page folio by sequence agreement across consecutive pages.
///
/// A candidate on page N is accepted if its numeric value is exactly one more
/// than the accepted value on the nearest preceding page that has one (arabic
/// and roman sequences are tracked independently since front matter and body
/// use different numerals). Where no candidate agrees, `folio` is `None`.
///
/// A correctly detected folio series has a constant `folio value - page
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
            PageRecord { page_index: i + 1, folio, text: text.clone() }
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
            breaks.push(FolioBreak::Gap { page_indices: std::mem::take(run) });
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
                page_index: record.page_index,
                folio: candidate.value.clone(),
            });
        } else {
            gap_run.push(record.page_index);
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

    fn anchor(s: &str) -> FolioAnchor {
        s.parse().expect("test anchor parses")
    }

    #[test]
    fn anchor_parses_page_and_folio() {
        assert_eq!(anchor("2=3"), FolioAnchor { page_index: 2, folio: "3".into() });
        assert_eq!(anchor(" 4 = xii "), FolioAnchor { page_index: 4, folio: "xii".into() });
    }

    #[test]
    fn anchor_rejects_malformed_input() {
        for bad in ["2", "0=1", "x=1", "2=", "2=III", "2=page3"] {
            assert!(bad.parse::<FolioAnchor>().is_err(), "{bad} should not parse");
        }
    }

    /// The case that motivated anchoring: a scan whose first leaf is folio 1
    /// and whose second is folio 3. No single offset describes it.
    #[test]
    fn anchors_describe_a_discontinuous_mapping() {
        let pages = vec![page("a"), page("b"), page("c"), page("d")];
        let recs = anchor_folios(&pages, &[anchor("1=1"), anchor("2=3")]);
        let folios: Vec<Option<&str>> = recs.iter().map(|r| r.folio.as_deref()).collect();
        assert_eq!(folios, vec![Some("1"), Some("3"), Some("4"), Some("5")]);
    }

    #[test]
    fn anchors_ignore_page_text_entirely() {
        let pages = vec![page("999\nbody\n999"), page("999\nbody\n999")];
        let recs = anchor_folios(&pages, &[anchor("1=7")]);
        assert_eq!(recs[0].folio.as_deref(), Some("7"));
        assert_eq!(recs[1].folio.as_deref(), Some("8"));
    }

    #[test]
    fn anchor_advances_roman_front_matter_in_roman() {
        let pages = vec![page("a"), page("b"), page("c")];
        let recs = anchor_folios(&pages, &[anchor("1=viii")]);
        let folios: Vec<Option<&str>> = recs.iter().map(|r| r.folio.as_deref()).collect();
        assert_eq!(folios, vec![Some("viii"), Some("ix"), Some("x")]);
    }

    #[test]
    fn pages_before_the_first_anchor_get_no_folio() {
        let pages = vec![page("a"), page("b"), page("c")];
        let recs = anchor_folios(&pages, &[anchor("2=1")]);
        let folios: Vec<Option<&str>> = recs.iter().map(|r| r.folio.as_deref()).collect();
        assert_eq!(folios, vec![None, Some("1"), Some("2")]);
    }

    #[test]
    fn anchors_apply_in_page_order_however_they_are_given() {
        let pages = vec![page("a"), page("b"), page("c"), page("d")];
        let forward = anchor_folios(&pages, &[anchor("1=1"), anchor("3=10")]);
        let reversed = anchor_folios(&pages, &[anchor("3=10"), anchor("1=1")]);
        assert_eq!(forward.iter().map(|r| r.folio.clone()).collect::<Vec<_>>(),
                   reversed.iter().map(|r| r.folio.clone()).collect::<Vec<_>>());
        assert_eq!(forward[3].folio.as_deref(), Some("11"));
    }

    #[test]
    fn folio_runs_break_where_the_sequence_jumps() {
        let pages = vec![page("a"), page("b"), page("c")];
        let recs = anchor_folios(&pages, &[anchor("1=1"), anchor("2=3")]);
        let runs = folio_runs(&recs);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].to_string(), "artifact-page 1 = folio 1");
        assert_eq!(runs[1].to_string(), "artifact-page 2-3 = folio 3-4");
    }

    #[test]
    fn folio_runs_collapse_a_clean_sequence_to_one_line() {
        let pages = vec![page("a"), page("b"), page("c")];
        let recs = anchor_folios(&pages, &[anchor("1=5")]);
        let runs = folio_runs(&recs);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].to_string(), "artifact-page 1-3 = folio 5-7");
    }

    #[test]
    fn validate_folios_reports_gap() {
        let records = vec![
            PageRecord { page_index: 1, folio: Some("7".into()), text: String::new() },
            PageRecord { page_index: 2, folio: None, text: String::new() },
            PageRecord { page_index: 3, folio: None, text: String::new() },
            PageRecord { page_index: 4, folio: Some("10".into()), text: String::new() },
        ];
        let breaks = validate_folios(&records);
        assert_eq!(breaks, vec![FolioBreak::Gap { page_indices: vec![2, 3] }]);
    }

    #[test]
    fn validate_folios_reports_no_breaks_for_clean_sequence() {
        let records = vec![
            PageRecord { page_index: 1, folio: Some("7".into()), text: String::new() },
            PageRecord { page_index: 2, folio: Some("8".into()), text: String::new() },
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
        assert_eq!(breaks, vec![FolioBreak::Contradiction { page_index: 2, folio: "999".into() }]);
    }

    #[test]
    fn form_feed_separator_splits_on_the_terminator() {
        let sep: PageSeparator = "form-feed".parse().unwrap();
        assert_eq!(sep.split("a\u{0c}b\u{0c}c"), vec!["a", "b", "c"]);
        // A trailing form feed (pdftotext's convention) yields no empty final page.
        assert_eq!(sep.split("a\u{0c}b\u{0c}"), vec!["a", "b"]);
    }

    #[test]
    fn blank_lines_separator_splits_on_the_threshold() {
        let sep: PageSeparator = "blank-lines:2".parse().unwrap();
        let pages = sep.split("page one\n\n\npage two\n");
        assert_eq!(pages.len(), 2);
        assert!(pages[0].contains("page one"));
        assert!(pages[1].contains("page two"));
        // A single blank line does not reach the threshold.
        assert_eq!(sep.split("a\n\nb").len(), 1);
    }

    #[test]
    fn regex_separator_starts_a_page_at_each_matching_line() {
        let sep: PageSeparator = "regex:^Page \\d+".parse().unwrap();
        let pages = sep.split("Page 1\nbody\nPage 2\nmore body\n");
        assert_eq!(pages.len(), 2);
        assert!(pages[0].starts_with("Page 1"));
        assert!(pages[1].starts_with("Page 2"));
    }

    #[test]
    fn separator_rejects_unknown_and_malformed_forms() {
        assert!("pages-by".parse::<PageSeparator>().is_err());
        assert!("blank-lines:0".parse::<PageSeparator>().is_err());
        assert!("regex:[".parse::<PageSeparator>().is_err());
    }
}
