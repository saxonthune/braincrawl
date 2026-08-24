use crate::fetch_content::is_valid_pdf;

/// Extract UTF-8 text from born-digital PDF bytes.
pub fn extract_text(bytes: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    if !is_valid_pdf(bytes) {
        return Err("bytes do not begin with %PDF magic — not a valid PDF".into());
    }
    let raw = catch_extractor_panic(|| pdf_extract::extract_text_from_mem(bytes))?;
    Ok(normalize(raw))
}

/// Extract UTF-8 text from born-digital PDF bytes, one normalized String per page.
pub fn extract_pages(bytes: &[u8]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if !is_valid_pdf(bytes) {
        return Err("bytes do not begin with %PDF magic — not a valid PDF".into());
    }
    let raw_pages = catch_extractor_panic(|| pdf_extract::extract_text_from_mem_by_pages(bytes))?;
    Ok(raw_pages.into_iter().map(normalize).collect())
}

/// Run a `pdf_extract` call, converting a panic into a controlled error.
///
/// `pdf_extract` 0.7 panics (index out of bounds at `lib.rs:1736`) on some PDFs
/// with unusual font/page structures instead of returning `Err`. Swap the panic
/// hook to a no-op around the call so no backtrace prints, then map the caught
/// unwind to an actionable error. Single-threaded callers only — the hook swap
/// is process-global.
fn catch_extractor_panic<T, E>(
    f: impl FnOnce() -> Result<T, E> + std::panic::UnwindSafe,
) -> Result<T, Box<dyn std::error::Error>>
where
    E: std::error::Error + 'static,
{
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let caught = std::panic::catch_unwind(f);
    std::panic::set_hook(prev);
    match caught {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(Box::new(e)),
        Err(_) => Err("could not extract text from this PDF: its internal font or page \
             structure is unsupported by the extractor. Supply text by hand with \
             `library put <id> <file.txt> --role text`."
            .into()),
    }
}

pub fn normalize(raw: String) -> String {
    let trimmed: Vec<&str> = raw.lines().map(|l| l.trim_end()).collect();
    let mut out = String::with_capacity(raw.len());
    let mut blank_run = 0usize;
    for line in &trimmed {
        if line.is_empty() {
            blank_run += 1;
            if blank_run <= 2 {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_trims_trailing_whitespace() {
        let raw = "hello   \nworld  \n".to_string();
        let result = normalize(raw);
        assert_eq!(result, "hello\nworld\n");
    }

    #[test]
    fn normalize_collapses_excess_blank_lines() {
        let raw = "a\n\n\n\n\nb\n".to_string();
        let result = normalize(raw);
        assert!(result.contains("a\n\n\nb") || result.contains("a\n\nb"));
        let blank_runs: usize = result
            .lines()
            .filter(|l| l.is_empty())
            .count();
        assert!(blank_runs <= 2, "expected at most 2 blank lines but got {blank_runs}");
    }

    #[test]
    fn normalize_keeps_single_blank_lines() {
        let raw = "a\n\nb\n".to_string();
        let result = normalize(raw);
        assert_eq!(result, "a\n\nb\n");
    }

    #[test]
    fn extract_text_rejects_non_pdf() {
        let err = extract_text(b"<html>not a pdf</html>").unwrap_err();
        assert!(err.to_string().contains("%PDF"));
    }

    #[test]
    fn extract_text_on_fixture() {
        let bytes = include_bytes!("../tests/fixtures/hello.pdf");
        let text = extract_text(bytes).expect("extraction should succeed");
        assert!(
            text.contains("Hello braincrawl"),
            "expected 'Hello braincrawl' in extracted text, got: {text:?}"
        );
    }

    #[test]
    fn extract_pages_on_fixture() {
        let bytes = include_bytes!("../tests/fixtures/hello.pdf");
        let pages = extract_pages(bytes).expect("paginated extraction should succeed");
        assert!(!pages.is_empty());
        assert!(pages.iter().any(|p| p.contains("Hello braincrawl")));
    }

    #[test]
    fn extract_pages_rejects_non_pdf() {
        let err = extract_pages(b"<html>not a pdf</html>").unwrap_err();
        assert!(err.to_string().contains("%PDF"));
    }

    #[test]
    fn catch_extractor_panic_converts_panic_to_error() {
        let err = catch_extractor_panic(|| -> Result<(), std::io::Error> {
            panic!("index out of bounds: the len is 0 but the index is 0")
        })
        .unwrap_err();
        assert!(err.to_string().contains("could not extract text"));
        assert!(err.to_string().contains("library put"));
    }

    #[test]
    fn catch_extractor_panic_passes_ok_through() {
        let ok = catch_extractor_panic(|| Ok::<_, std::io::Error>(42)).unwrap();
        assert_eq!(ok, 42);
    }
}
