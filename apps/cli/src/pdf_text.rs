use crate::fetch_content::is_valid_pdf;

/// Extract UTF-8 text from born-digital PDF bytes.
pub fn extract_text(bytes: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    if !is_valid_pdf(bytes) {
        return Err("bytes do not begin with %PDF magic — not a valid PDF".into());
    }
    let raw = pdf_extract::extract_text_from_mem(bytes)?;
    Ok(normalize(raw))
}

fn normalize(raw: String) -> String {
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
}
