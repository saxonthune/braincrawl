//! `braincrawl web` — prints the Web UI URL for the configured server.

use crate::config::Config;

/// Print `<server_url>/web` to stdout. No browser launching, no health probing.
pub fn print_url(config: &Config) {
    println!("{}/web", config.server_url);
}
