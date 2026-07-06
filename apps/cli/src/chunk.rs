use text_splitter::{ChunkConfig, ChunkSizer, TextSplitter};
use tiktoken_rs::CoreBPE;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChunkRecord {
    pub seq: usize,
    pub text: String,
    pub page_start: usize,
    pub page_end: usize,
    pub token_count: usize,
    pub section_path: Vec<String>,
}

/// Minimum non-whitespace chars for a page to count as having a text layer.
const FAITHFUL_MIN_CHARS: usize = 5;

/// Return the 1-indexed page numbers whose normalized text has no text layer
/// (fewer than `FAITHFUL_MIN_CHARS` non-whitespace chars) — i.e. scanned/image-only pages.
pub fn unfaithful_pages(pages: &[String]) -> Vec<usize> {
    pages
        .iter()
        .enumerate()
        .filter(|(_, text)| text.chars().filter(|c| !c.is_whitespace()).count() < FAITHFUL_MIN_CHARS)
        .map(|(i, _)| i + 1)
        .collect()
}

/// Partition `pages` (one normalized string per page, 1-indexed by position) into
/// citation-carrying chunks. A page with empty text (e.g. a skipped no-text-layer
/// page) contributes no text but keeps its page number in the numbering.
pub fn chunk_pages(pages: &[String], max_tokens: usize, overlap: usize) -> Vec<ChunkRecord> {
    if pages.is_empty() {
        return Vec::new();
    }

    let mut joined = String::new();
    let mut page_ranges: Vec<(usize, usize, usize)> = Vec::with_capacity(pages.len());
    for (i, page) in pages.iter().enumerate() {
        let start = joined.len();
        joined.push_str(page);
        let end = joined.len();
        page_ranges.push((i + 1, start, end));
    }

    if joined.trim().is_empty() {
        return Vec::new();
    }

    let tokenizer: CoreBPE = tiktoken_rs::cl100k_base().expect("cl100k_base tokenizer ranks are bundled with tiktoken-rs");
    let sizer = tokenizer.clone();
    let config = ChunkConfig::new(max_tokens)
        .with_sizer(tokenizer)
        .with_overlap(overlap)
        .expect("overlap must be smaller than max_tokens");
    let splitter = TextSplitter::new(config);

    splitter
        .chunk_indices(&joined)
        .enumerate()
        .map(|(seq, (offset, text))| {
            let end_offset = offset + text.len();
            let last_byte = end_offset.saturating_sub(1).max(offset);
            ChunkRecord {
                seq,
                token_count: sizer.size(text),
                page_start: page_for_offset(&page_ranges, offset),
                page_end: page_for_offset(&page_ranges, last_byte),
                text: text.to_string(),
                section_path: Vec::new(),
            }
        })
        .collect()
}

/// Map a byte offset in the joined text to its 1-indexed page number.
fn page_for_offset(page_ranges: &[(usize, usize, usize)], offset: usize) -> usize {
    for &(page_num, start, end) in page_ranges {
        if offset >= start && offset < end {
            return page_num;
        }
    }
    page_ranges.last().map(|&(page_num, _, _)| page_num).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfaithful_pages_flags_blank_and_whitespace_only() {
        let pages = vec![
            "Real content on this page.".to_string(),
            "   \n\n  ".to_string(),
            "".to_string(),
            "abcd".to_string(),
        ];
        assert_eq!(unfaithful_pages(&pages), vec![2, 3, 4]);
    }

    #[test]
    fn chunk_pages_assigns_page_numbers() {
        let pages = vec!["one two three four five. ".repeat(50), "six seven eight nine ten. ".repeat(50)];
        let chunks = chunk_pages(&pages, 64, 8);
        assert!(!chunks.is_empty());
        assert!(chunks[0].page_start >= 1);
        assert!(chunks.last().unwrap().page_end <= 2);
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(c.seq, i);
        }
    }

    #[test]
    fn chunk_pages_skips_empty_pages_without_shifting_numbers() {
        let pages = vec![
            "Content on page one.".repeat(20),
            String::new(),
            "Content on page three.".repeat(20),
        ];
        let chunks = chunk_pages(&pages, 32, 4);
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.page_start != 2 && c.page_end != 2));
        assert!(chunks.iter().any(|c| c.page_start == 3 || c.page_end == 3));
    }

    #[test]
    fn chunk_pages_empty_input_returns_empty() {
        assert!(chunk_pages(&[], 512, 64).is_empty());
    }
}
