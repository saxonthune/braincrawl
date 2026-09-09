use std::path::{Path, PathBuf};

use deunicode::deunicode;

use crate::cli::{OutputOpts, RenameArgs};

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "of", "on", "in", "at", "to", "for", "and", "or", "nor", "but", "is",
    "are", "was", "were", "be", "been", "being", "with", "from", "by", "as", "into",
];

/// CamelCase an author surname: transliterate to ASCII, split on whitespace/hyphens,
/// capitalize each part, strip remaining non-alphanumeric characters.
fn surname_key(author: &str) -> String {
    deunicode(author)
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars().filter(|c| c.is_alphanumeric());
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let mut s: String = first.to_uppercase().collect();
                    s.extend(chars.flat_map(|c| c.to_lowercase()));
                    s
                }
            }
        })
        .collect()
}

/// Slugify a title: transliterate to ASCII, lowercase, tokenize on non-alphanumeric
/// boundaries, drop stopwords, keep the first 5 surviving tokens, join with `-`.
fn title_slug(title: &str) -> String {
    let ascii = deunicode(title).to_lowercase();
    let tokens: Vec<&str> = ascii
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();

    let surviving: Vec<&str> = tokens
        .iter()
        .filter(|t| !STOPWORDS.contains(t))
        .take(5)
        .copied()
        .collect();

    if !surviving.is_empty() {
        return surviving.join("-");
    }
    match tokens.first() {
        Some(t) => t.to_string(),
        None => "untitled".to_string(),
    }
}

fn stem(surname_key: &str, et_al: bool, year: u32, title_slug: &str) -> String {
    let marker = if et_al { "EtAl" } else { "" };
    format!("{surname_key}{marker}{year}-{title_slug}")
}

/// Resolve the final filename for `stem.ext` in `dir`, avoiding collisions with
/// existing files other than `current` (the file being renamed, for idempotency).
fn resolve_collision(dir: &Path, stem: &str, ext: &str, current: &Path) -> String {
    let filename = |suffix: &str| -> String {
        if ext.is_empty() {
            format!("{stem}{suffix}")
        } else {
            format!("{stem}{suffix}.{ext}")
        }
    };

    let is_free = |name: &str| -> bool {
        let candidate = dir.join(name);
        !candidate.exists() || candidate == current
    };

    let bare = filename("");
    if is_free(&bare) {
        return bare;
    }

    for letter in b'b'..=b'z' {
        let candidate_name = filename(&(letter as char).to_string());
        if is_free(&candidate_name) {
            return candidate_name;
        }
    }

    unreachable!("exhausted collision suffixes a-z")
}

pub fn run(args: RenameArgs, opts: &OutputOpts) -> Result<(), Box<dyn std::error::Error>> {
    let original = Path::new(&args.file);
    if !original.exists() {
        return Err(format!("no such file: {}", original.display()).into());
    }
    let original = std::fs::canonicalize(original)?;

    let dir = original
        .parent()
        .ok_or_else(|| format!("cannot determine parent directory of {}", original.display()))?;
    let ext = original
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let surname = surname_key(&args.author);
    let slug = title_slug(&args.title);
    let file_stem = stem(&surname, args.et_al, args.year, &slug);
    let new_filename = resolve_collision(dir, &file_stem, &ext, &original);
    let new_path: PathBuf = dir.join(&new_filename);

    if args.dry_run {
        print_path(&new_path, opts);
        return Ok(());
    }

    std::fs::rename(&original, &new_path)?;
    print_path(&new_path, opts);
    Ok(())
}

fn print_path(path: &Path, opts: &OutputOpts) {
    if opts.json {
        let out = serde_json::json!({ "path": path.display().to_string() });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        println!("{}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surname_key_solo_lowercase() {
        assert_eq!(surname_key("piwowar"), "Piwowar");
    }

    #[test]
    fn surname_key_particles() {
        assert_eq!(surname_key("Van De Mieroop"), "VanDeMieroop");
    }

    #[test]
    fn surname_key_diacritics() {
        assert_eq!(surname_key("Şenyurt"), "Senyurt");
    }

    #[test]
    fn surname_key_hyphenated() {
        assert_eq!(surname_key("García-López"), "GarciaLopez");
    }

    #[test]
    fn title_slug_stopwords_and_punctuation() {
        assert_eq!(
            title_slug("The state of OA: a large-scale analysis"),
            "state-oa-large-scale-analysis"
        );
    }

    #[test]
    fn title_slug_diacritics_fold_to_ascii() {
        assert_eq!(title_slug("Şenyurt's Ünal Study"), "senyurt-s-unal-study");
    }

    #[test]
    fn title_slug_all_stopwords_falls_back_to_first_token() {
        assert_eq!(title_slug("The of a"), "the");
    }

    #[test]
    fn title_slug_empty_falls_back_to_untitled() {
        assert_eq!(title_slug("   "), "untitled");
    }

    #[test]
    fn stem_solo_author() {
        assert_eq!(stem("Piwowar", false, 2018, "state-oa"), "Piwowar2018-state-oa");
    }

    #[test]
    fn stem_et_al() {
        assert_eq!(
            stem("Piwowar", true, 2018, "state-oa"),
            "PiwowarEtAl2018-state-oa"
        );
    }

    #[test]
    fn resolve_collision_bare_stem_when_free() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("does-not-exist.pdf");
        let name = resolve_collision(dir.path(), "Piwowar2018-state-oa", "pdf", &current);
        assert_eq!(name, "Piwowar2018-state-oa.pdf");
    }

    #[test]
    fn resolve_collision_bumps_to_b_when_bare_taken() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Piwowar2018-state-oa.pdf"), b"x").unwrap();
        let current = dir.path().join("does-not-exist.pdf");
        let name = resolve_collision(dir.path(), "Piwowar2018-state-oa", "pdf", &current);
        assert_eq!(name, "Piwowar2018-state-oab.pdf");
    }

    #[test]
    fn resolve_collision_idempotent_when_current_is_bare_stem() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("Piwowar2018-state-oa.pdf");
        std::fs::write(&current, b"x").unwrap();
        let name = resolve_collision(dir.path(), "Piwowar2018-state-oa", "pdf", &current);
        assert_eq!(name, "Piwowar2018-state-oa.pdf");
    }
}
