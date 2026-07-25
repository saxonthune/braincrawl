//! `braincrawl doctor` — a registry of read-only checks reporting drift across
//! the CLI binary, the server it talks to, the config that points at it, and
//! the store behind it. Adding a watched thing means adding one `Check` to
//! `run`; nothing here mutates anything.

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::cli::OutputOpts;
use crate::config::Config;
use crate::store_client::StoreClient;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Fail,
    /// A fact that cannot fail, such as store identity or build stamps.
    Info,
}

impl Status {
    fn marker(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
            Status::Info => "INFO",
        }
    }
}

pub struct Check {
    pub name: &'static str,
    pub status: Status,
    pub detail: String,
    /// Printed only when `status` is not `Ok` — e.g. `Some("just upgrade")`.
    pub remedy: Option<&'static str>,
}

/// Run every registry entry in order and return the results.
pub fn run(config: &Config) -> Vec<Check> {
    let client = StoreClient::new(&config.server_url).with_token(config.auth_token.clone());
    let health = client.health();
    let server_ok = health.is_ok();

    let mut checks = Vec::new();

    checks.push(server_reachable_check(config, &health));
    checks.push(server_url_check(config));
    checks.push(cli_build_check());

    let cli_build = env!("BRAINCRAWL_BUILD");
    let server_build = health.as_ref().ok().and_then(|v| v["version"].as_str()).map(str::to_string);
    checks.push(server_build_check(server_ok, server_build.as_deref()));
    checks.push(build_match_check(server_ok, cli_build, server_build.as_deref()));

    checks.push(store_identity_check(server_ok, &client));
    checks.push(l3_root_check(config, server_ok, &health));

    let applied = health_string_array(&health, "migrations_applied");
    let compiled = health_string_array(&health, "migrations_compiled");
    checks.push(migrations_check(server_ok, &applied, &compiled));

    checks.push(config_source_check());

    checks
}

fn health_string_array(health: &Result<serde_json::Value, crate::store_client::ClientError>, key: &str) -> Vec<String> {
    health
        .as_ref()
        .ok()
        .and_then(|v| v[key].as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn server_reachable_check(
    config: &Config,
    health: &Result<serde_json::Value, crate::store_client::ClientError>,
) -> Check {
    match health {
        Ok(_) => Check {
            name: "server-reachable",
            status: Status::Ok,
            detail: format!("GET {}/health responded", config.server_url),
            remedy: None,
        },
        Err(e) => Check {
            name: "server-reachable",
            status: Status::Fail,
            detail: format!("GET {}/health failed: {e}", config.server_url),
            remedy: Some("start the server, or check server_url"),
        },
    }
}

fn server_url_check(config: &Config) -> Check {
    let sources = Config::sources();
    let source = sources.iter().find(|f| f.name == "server_url").map(|f| f.source).unwrap_or("default");
    Check {
        name: "server-url",
        status: Status::Info,
        detail: format!("{} (from {source})", config.server_url),
        remedy: None,
    }
}

fn cli_build_check() -> Check {
    Check {
        name: "cli-build",
        status: Status::Info,
        detail: env!("BRAINCRAWL_BUILD").to_string(),
        remedy: None,
    }
}

fn server_build_check(server_ok: bool, server_build: Option<&str>) -> Check {
    if !server_ok {
        return Check {
            name: "server-build",
            status: Status::Fail,
            detail: "server unreachable".to_string(),
            remedy: None,
        };
    }
    match server_build {
        Some(v) => Check { name: "server-build", status: Status::Info, detail: v.to_string(), remedy: None },
        None => Check {
            name: "server-build",
            status: Status::Warn,
            detail: "(absent — server predates the build stamp)".to_string(),
            remedy: Some("just upgrade"),
        },
    }
}

/// `Ok` when the CLI and server build stamps match, `Warn` otherwise (including
/// when the server's stamp is unknown). Pure so it can be unit-tested directly.
fn build_match_status(cli_build: &str, server_build: Option<&str>) -> Status {
    match server_build {
        Some(s) if s == cli_build => Status::Ok,
        _ => Status::Warn,
    }
}

fn build_match_check(server_ok: bool, cli_build: &str, server_build: Option<&str>) -> Check {
    if !server_ok {
        return Check {
            name: "build-match",
            status: Status::Fail,
            detail: "server unreachable".to_string(),
            remedy: None,
        };
    }
    let status = build_match_status(cli_build, server_build);
    let detail = match server_build {
        Some(s) => format!("cli={cli_build} server={s}"),
        None => format!("cli={cli_build} server=(unknown)"),
    };
    Check { name: "build-match", status, detail, remedy: if status == Status::Ok { None } else { Some("just upgrade") } }
}

fn store_identity_check(server_ok: bool, client: &StoreClient) -> Check {
    if !server_ok {
        return Check {
            name: "store-identity",
            status: Status::Fail,
            detail: "server unreachable".to_string(),
            remedy: None,
        };
    }
    match client.stats() {
        Ok(v) => Check {
            name: "store-identity",
            status: Status::Info,
            detail: format!(
                "works={} edges={} library_bytes={} catalog_bytes={}",
                v["works"].as_u64().unwrap_or(0),
                v["edges_total"].as_u64().unwrap_or(0),
                v["library_bytes"].as_u64().unwrap_or(0),
                v["catalog_bytes"].as_u64().unwrap_or(0),
            ),
            remedy: None,
        },
        Err(e) => Check {
            name: "store-identity",
            status: Status::Fail,
            detail: format!("GET /stats failed: {e}"),
            remedy: None,
        },
    }
}

fn l3_root_check(config: &Config, server_ok: bool, health: &Result<serde_json::Value, crate::store_client::ClientError>) -> Check {
    if !server_ok {
        return Check { name: "l3-root", status: Status::Fail, detail: "server unreachable".to_string(), remedy: None };
    }
    let cli_root = config.l3_root().display().to_string();
    let server_root = health.as_ref().ok().and_then(|v| v["l3_root"].as_str().map(str::to_string));
    match server_root {
        None => Check {
            name: "l3-root",
            status: Status::Info,
            detail: format!("cli={cli_root} server=(unset)"),
            remedy: None,
        },
        Some(server_root) if server_root == cli_root => Check {
            name: "l3-root",
            status: Status::Info,
            detail: format!("cli={cli_root} server={server_root}"),
            remedy: None,
        },
        Some(server_root) => Check {
            name: "l3-root",
            status: Status::Warn,
            detail: format!("cli={cli_root} server={server_root}"),
            remedy: Some("point BRAINCRAWL_L3_REPO / --l3-repo at the server's l3_root, or vice versa"),
        },
    }
}

/// `Ok` when the applied and compiled migration sets match, `Warn` when the
/// database has migrations the binary doesn't know (server built newer than
/// this binary), `Fail` when the binary has migrations the database lacks
/// (this binary hasn't run against the database yet). Pure so it can be
/// unit-tested directly.
fn migrations_status(applied: &[String], compiled: &[String]) -> Status {
    let applied_set: HashSet<&String> = applied.iter().collect();
    let compiled_set: HashSet<&String> = compiled.iter().collect();
    if applied_set == compiled_set {
        Status::Ok
    } else if compiled_set.iter().any(|c| !applied_set.contains(c)) {
        Status::Fail
    } else {
        Status::Warn
    }
}

fn migrations_check(server_ok: bool, applied: &[String], compiled: &[String]) -> Check {
    if !server_ok {
        return Check {
            name: "migrations",
            status: Status::Fail,
            detail: "server unreachable".to_string(),
            remedy: None,
        };
    }
    let status = migrations_status(applied, compiled);
    Check {
        name: "migrations",
        status,
        detail: format!("applied={} compiled={}", applied.len(), compiled.len()),
        remedy: if status == Status::Ok { None } else { Some("just upgrade") },
    }
}

fn config_source_check() -> Check {
    let sources = Config::sources();
    let mut detail = String::new();
    for (i, f) in sources.iter().enumerate() {
        if i > 0 {
            detail.push(' ');
        }
        let _ = write!(detail, "{}={}({})", f.name, f.source, if f.set { "set" } else { "unset" });
    }
    Check { name: "config-source", status: Status::Info, detail, remedy: None }
}

/// Render checks to stdout per the global output flags:
/// `--json` an array of objects, `--text` one tab-separated line per check,
/// neither the default human-readable form (marker, name, detail, indented remedy).
pub fn render(checks: &[Check], opts: &OutputOpts) {
    if opts.json {
        let value: Vec<serde_json::Value> = checks
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "status": c.status,
                    "detail": c.detail,
                    "remedy": c.remedy,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
    } else if opts.text {
        for c in checks {
            println!(
                "{}\t{:?}\t{}\t{}",
                c.name,
                c.status,
                c.detail,
                c.remedy.unwrap_or(""),
            );
        }
    } else {
        let mut out = String::new();
        for c in checks {
            let _ = writeln!(out, "{:<4} {:<18} {}", c.status.marker(), c.name, c.detail);
            if c.status != Status::Ok {
                if let Some(remedy) = c.remedy {
                    let _ = writeln!(out, "    remedy: {remedy}");
                }
            }
        }
        print!("{out}");
    }
}

/// `true` when any check `Fail`ed — the CLI exits non-zero on this.
pub fn any_failed(checks: &[Check]) -> bool {
    checks.iter().any(|c| c.status == Status::Fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_match_equal_stamps_is_ok() {
        assert_eq!(build_match_status("v1.2.3", Some("v1.2.3")), Status::Ok);
    }

    #[test]
    fn build_match_differing_stamps_is_warn() {
        assert_eq!(build_match_status("v1.2.3", Some("v1.2.2")), Status::Warn);
    }

    #[test]
    fn build_match_unknown_server_stamp_is_warn() {
        assert_eq!(build_match_status("v1.2.3", None), Status::Warn);
    }

    #[test]
    fn migrations_equal_sets_is_ok() {
        let a = vec!["0001_init".to_string(), "0002_artifacts".to_string()];
        let b = a.clone();
        assert_eq!(migrations_status(&a, &b), Status::Ok);
    }

    #[test]
    fn migrations_database_ahead_is_warn() {
        let applied = vec!["0001_init".to_string(), "0002_artifacts".to_string()];
        let compiled = vec!["0001_init".to_string()];
        assert_eq!(migrations_status(&applied, &compiled), Status::Warn);
    }

    #[test]
    fn migrations_binary_ahead_is_fail() {
        let applied = vec!["0001_init".to_string()];
        let compiled = vec!["0001_init".to_string(), "0002_artifacts".to_string()];
        assert_eq!(migrations_status(&applied, &compiled), Status::Fail);
    }
}
