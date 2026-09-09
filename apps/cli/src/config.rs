use std::collections::BTreeMap;
use std::path::PathBuf;

/// One named store from the config's `[stores.<name>]` tables: a base URL and
/// the token that belongs to it. Bare URLs given on the command line become an
/// unnamed `StoreRef`.
#[derive(Debug, Clone)]
pub struct StoreRef {
    pub name: Option<String>,
    pub url: String,
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub server_url: String,
    pub openalex_api_key: Option<String>,
    pub semanticscholar_api_key: Option<String>,
    pub auth_token: Option<String>,
    pub unpaywall_email: Option<String>,
    pub crossref_mailto: Option<String>,
    /// Root directory of the consolidated L3 document store.
    pub l3_repo: Option<String>,
    /// Named stores from `[stores.<name>]`; empty when none are configured.
    pub stores: BTreeMap<String, StoreRef>,
    /// Name of the store every command targets by default (`active_store`).
    pub active_store: Option<String>,
}

#[derive(serde::Deserialize, Default, Clone)]
struct StoreEntry {
    url: Option<String>,
    auth_token: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct ConfigFile {
    server_url: Option<String>,
    openalex_api_key: Option<String>,
    semanticscholar_api_key: Option<String>,
    auth_token: Option<String>,
    unpaywall_email: Option<String>,
    crossref_mailto: Option<String>,
    l3_repo: Option<String>,
    active_store: Option<String>,
    stores: Option<BTreeMap<String, StoreEntry>>,
}

impl Config {
    /// Resolve config with precedence: env > active named store > flat file
    /// keys > default. A caller may override `server_url` before use
    /// (flag > env > active store > file > default).
    pub fn resolve() -> Self {
        let file = load_config_file();
        let stores: BTreeMap<String, StoreRef> = file
            .stores
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(name, e)| {
                e.url.map(|url| {
                    (
                        name.clone(),
                        StoreRef { name: Some(name), url, auth_token: e.auth_token },
                    )
                })
            })
            .collect();
        let active = file
            .active_store
            .as_ref()
            .and_then(|name| stores.get(name))
            .cloned();
        Config {
            server_url: std::env::var("BRAINCRAWL_SERVER_URL")
                .ok()
                .or_else(|| active.as_ref().map(|s| s.url.clone()))
                .or(file.server_url)
                .unwrap_or_else(|| "http://127.0.0.1:8787".to_string()),
            openalex_api_key: std::env::var("BRAINCRAWL_OPENALEX_API_KEY")
                .ok()
                .or(file.openalex_api_key),
            semanticscholar_api_key: std::env::var("BRAINCRAWL_SEMANTICSCHOLAR_API_KEY")
                .ok()
                .or(file.semanticscholar_api_key),
            auth_token: std::env::var("BRAINCRAWL_AUTH_TOKEN")
                .ok()
                .or_else(|| active.as_ref().and_then(|s| s.auth_token.clone()))
                .or(file.auth_token),
            unpaywall_email: std::env::var("BRAINCRAWL_UNPAYWALL_EMAIL")
                .ok()
                .or(file.unpaywall_email),
            crossref_mailto: std::env::var("BRAINCRAWL_CROSSREF_MAILTO")
                .ok()
                .or(file.crossref_mailto),
            l3_repo: std::env::var("BRAINCRAWL_L3_REPO").ok().or(file.l3_repo),
            stores,
            active_store: file.active_store,
        }
    }

    /// Resolve a store spec — a configured store name, or a bare base URL — to
    /// a `StoreRef`. A URL that matches a named store's URL borrows that
    /// store's token.
    pub fn store_ref(&self, spec: &str) -> Result<StoreRef, String> {
        if spec.contains("://") {
            let url = spec.trim_end_matches('/').to_string();
            let named = self
                .stores
                .values()
                .find(|s| s.url.trim_end_matches('/') == url);
            return Ok(StoreRef {
                name: named.and_then(|s| s.name.clone()),
                url,
                auth_token: named.and_then(|s| s.auth_token.clone()),
            });
        }
        self.stores.get(spec).cloned().ok_or_else(|| {
            let known: Vec<&str> = self.stores.keys().map(|s| s.as_str()).collect();
            format!(
                "unknown store '{spec}' (configured stores: {}; or pass a base URL)",
                if known.is_empty() { "none".to_string() } else { known.join(", ") }
            )
        })
    }

    /// The active store as a `StoreRef`, falling back to the flat
    /// `server_url`/`auth_token` when no named store is active.
    pub fn active_store_ref(&self) -> StoreRef {
        if let Some(s) = self.active_store.as_ref().and_then(|n| self.stores.get(n)) {
            return s.clone();
        }
        StoreRef {
            name: None,
            url: self.server_url.clone(),
            auth_token: self.auth_token.clone(),
        }
    }

    /// Persist `active_store = "<name>"` into the config file, preserving the
    /// rest of the file verbatim. Only the one top-level line is rewritten; a
    /// missing line is inserted before the first `[table]` header so it stays
    /// a top-level key.
    pub fn set_active_store(name: &str) -> Result<PathBuf, String> {
        let path = config_file_path().ok_or("cannot determine config file path (no HOME)")?;
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let line = format!("active_store = \"{name}\"");
        let mut out = String::with_capacity(content.len() + line.len() + 1);
        let mut replaced = false;
        for l in content.lines() {
            if !replaced && l.trim_start().starts_with("active_store") {
                out.push_str(&line);
                replaced = true;
            } else {
                out.push_str(l);
            }
            out.push('\n');
        }
        if !replaced {
            let insert_at = content
                .lines()
                .take_while(|l| !l.trim_start().starts_with('['))
                .map(|l| l.len() + 1)
                .sum::<usize>()
                .min(out.len());
            out.insert_str(insert_at, &format!("{line}\n"));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, out).map_err(|e| e.to_string())?;
        Ok(path)
    }

    /// Resolve the L3 store root: configured `l3_repo` > `$HOME/.local/share/braincrawl/l3`.
    pub fn l3_root(&self) -> PathBuf {
        if let Some(p) = &self.l3_repo {
            return PathBuf::from(shellexpand_home(p));
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".local/share/braincrawl/l3")
    }

    /// Override the server URL (flag tier of the precedence chain).
    pub fn with_server_url(mut self, url: String) -> Self {
        self.server_url = url;
        self
    }

    /// Where each config field's effective value came from — env, the config
    /// file, or the default — and whether it's set at all. Never carries the
    /// value itself, so it's safe to print for secrets like `auth_token`.
    pub fn sources() -> Vec<FieldSource> {
        let file = load_config_file();
        vec![
            field_source("server_url", "BRAINCRAWL_SERVER_URL", file.server_url.as_deref(), true),
            field_source("auth_token", "BRAINCRAWL_AUTH_TOKEN", file.auth_token.as_deref(), false),
            field_source("openalex_api_key", "BRAINCRAWL_OPENALEX_API_KEY", file.openalex_api_key.as_deref(), false),
            field_source(
                "semanticscholar_api_key",
                "BRAINCRAWL_SEMANTICSCHOLAR_API_KEY",
                file.semanticscholar_api_key.as_deref(),
                false,
            ),
            field_source("unpaywall_email", "BRAINCRAWL_UNPAYWALL_EMAIL", file.unpaywall_email.as_deref(), false),
            field_source("crossref_mailto", "BRAINCRAWL_CROSSREF_MAILTO", file.crossref_mailto.as_deref(), false),
            field_source("l3_repo", "BRAINCRAWL_L3_REPO", file.l3_repo.as_deref(), false),
        ]
    }
}

/// Where one config field's effective value came from.
pub struct FieldSource {
    pub name: &'static str,
    pub source: &'static str,
    pub set: bool,
}

/// `has_default` marks fields (just `server_url`) that fall back to a real
/// default rather than staying unset when neither env nor file provide one.
fn field_source(name: &'static str, env_var: &str, file_value: Option<&str>, has_default: bool) -> FieldSource {
    if std::env::var(env_var).is_ok() {
        FieldSource { name, source: "env", set: true }
    } else if file_value.is_some() {
        FieldSource { name, source: "file", set: true }
    } else {
        FieldSource { name, source: "default", set: has_default }
    }
}

fn load_config_file() -> ConfigFile {
    let Some(path) = config_file_path() else {
        return ConfigFile::default();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return ConfigFile::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// Expand a leading `~/` or bare `~` to `$HOME`. Other shell expansions are not handled.
fn shellexpand_home(p: &str) -> String {
    if p == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| p.to_string());
    }
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    p.to_string()
}

fn config_file_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BRAINCRAWL_CONFIG") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/braincrawl/config.toml"))
}
