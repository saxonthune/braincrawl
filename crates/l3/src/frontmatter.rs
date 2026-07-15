//! String-level frontmatter upsert (ported from `apps/cli/src/l3.rs`, which
//! now calls through here so there is one implementation).

fn split_frontmatter(content: &str) -> (Vec<String>, String) {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return (Vec::new(), content.to_string());
    }
    let mut fm = Vec::new();
    let mut rest = Vec::new();
    let mut in_fm = true;
    for line in lines {
        if in_fm && line == "---" {
            in_fm = false;
            continue;
        }
        if in_fm {
            fm.push(line.to_string());
        } else {
            rest.push(line);
        }
    }
    if in_fm {
        return (Vec::new(), content.to_string());
    }
    let mut body = rest.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    (fm, body)
}

fn upsert_key(mut fm: Vec<String>, key: &str, value: &str) -> Vec<String> {
    let needle = format!("{key}: ");
    let bare = format!("{key}:");
    for line in fm.iter_mut() {
        if line.starts_with(&needle) || line == &bare {
            *line = format!("{key}: {value}");
            return fm;
        }
    }
    fm.push(format!("{key}: {value}"));
    fm
}

/// Set `key` to `value` in `content`'s frontmatter block: replace the key's
/// line if present, else insert it. If `content` has no frontmatter block,
/// one is created (wrapping the whole original content as the body).
pub fn upsert_frontmatter_key(content: &str, key: &str, value: &str) -> String {
    let (fm, body) = split_frontmatter(content);
    let fm = upsert_key(fm, key, value);
    format!("---\n{}\n---\n{body}", fm.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_key() {
        let content = "---\ndoc: old\nupdated: 2026-01-01\n---\n\nbody\n";
        let out = upsert_frontmatter_key(content, "doc", "new");
        let (fm, body) = split_frontmatter(&out);
        assert!(fm.contains(&"doc: new".to_string()));
        assert!(fm.contains(&"updated: 2026-01-01".to_string()));
        assert_eq!(body, "\nbody\n");
    }

    #[test]
    fn inserts_missing_key() {
        let content = "---\ndoc: d\n---\n\nbody\n";
        let out = upsert_frontmatter_key(content, "updated", "2026-07-14");
        let (fm, _) = split_frontmatter(&out);
        assert!(fm.contains(&"doc: d".to_string()));
        assert!(fm.contains(&"updated: 2026-07-14".to_string()));
    }

    #[test]
    fn creates_frontmatter_block_when_absent() {
        let content = "# L3 — No frontmatter\n\nbody openalex:W1\n";
        let out = upsert_frontmatter_key(content, "doc", "d");
        assert!(out.starts_with("---\ndoc: d\n---\n"));
        assert!(out.contains("# L3 — No frontmatter\n\nbody openalex:W1\n"));
    }
}
