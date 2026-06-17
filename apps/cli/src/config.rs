use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub server_url: String,
    pub openalex_api_key: Option<String>,
    pub semanticscholar_api_key: Option<String>,
    pub auth_token: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct ConfigFile {
    server_url: Option<String>,
    openalex_api_key: Option<String>,
    semanticscholar_api_key: Option<String>,
    auth_token: Option<String>,
}

impl Config {
    /// Resolve config with precedence: env > file > default.
    /// A caller may override `server_url` before use (flag > env > file > default).
    pub fn resolve() -> Self {
        let file = load_config_file();
        Config {
            server_url: std::env::var("BRAINCRAWL_SERVER_URL")
                .ok()
                .or_else(|| file.server_url)
                .unwrap_or_else(|| "http://127.0.0.1:8787".to_string()),
            openalex_api_key: std::env::var("BRAINCRAWL_OPENALEX_API_KEY")
                .ok()
                .or_else(|| file.openalex_api_key),
            semanticscholar_api_key: std::env::var("BRAINCRAWL_SEMANTICSCHOLAR_API_KEY")
                .ok()
                .or_else(|| file.semanticscholar_api_key),
            auth_token: std::env::var("BRAINCRAWL_AUTH_TOKEN")
                .ok()
                .or_else(|| file.auth_token),
        }
    }

    /// Override the server URL (flag tier of the precedence chain).
    pub fn with_server_url(mut self, url: String) -> Self {
        self.server_url = url;
        self
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

fn config_file_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BRAINCRAWL_CONFIG") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/braincrawl/config.toml"))
}
