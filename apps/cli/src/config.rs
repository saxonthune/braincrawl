use std::path::PathBuf;

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
}

impl Config {
    /// Resolve config with precedence: env > file > default.
    /// A caller may override `server_url` before use (flag > env > file > default).
    pub fn resolve() -> Self {
        let file = load_config_file();
        Config {
            server_url: std::env::var("BRAINCRAWL_SERVER_URL")
                .ok()
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
                .or(file.auth_token),
            unpaywall_email: std::env::var("BRAINCRAWL_UNPAYWALL_EMAIL")
                .ok()
                .or(file.unpaywall_email),
            crossref_mailto: std::env::var("BRAINCRAWL_CROSSREF_MAILTO")
                .ok()
                .or(file.crossref_mailto),
            l3_repo: std::env::var("BRAINCRAWL_L3_REPO").ok().or(file.l3_repo),
        }
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
