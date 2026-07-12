//! Native entry point for braincrawl-server.
//!
//! ## Configuration (env vars)
//!
//! | Variable                    | Default          | Description                                          |
//! |-----------------------------|------------------|------------------------------------------------------|
//! | `BRAINCRAWL_DB`             | `braincrawl.db`  | Path to the SQLite database file                     |
//! | `BRAINCRAWL_BLOB_ROOT`      | `blobs`          | Directory for filesystem blob store                  |
//! | `BRAINCRAWL_BIND`           | `0.0.0.0:8787`   | TCP bind address (default port 8787)                 |
//! | `BRAINCRAWL_AUTH_TOKEN`     | —                | Shared bearer token; required unless DISABLED is set |
//! | `BRAINCRAWL_AUTH_DISABLED`  | —                | Set to any non-empty value to bypass auth (dev only) |

use std::sync::Arc;

use braincrawl_auth::SharedSecret;
use braincrawl_server_lib::{make_app, make_store, AuthConfig};
use tokio::net::TcpListener;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let db_path =
        std::env::var("BRAINCRAWL_DB").unwrap_or_else(|_| "braincrawl.db".to_string());
    let blob_root =
        std::env::var("BRAINCRAWL_BLOB_ROOT").unwrap_or_else(|_| "blobs".to_string());
    let bind_addr =
        std::env::var("BRAINCRAWL_BIND").unwrap_or_else(|_| "0.0.0.0:8787".to_string());

    let auth = if std::env::var("BRAINCRAWL_AUTH_DISABLED")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        Arc::new(AuthConfig {
            disabled: true,
            allowlist: SharedSecret::new("", ""),
        })
    } else {
        let token = std::env::var("BRAINCRAWL_AUTH_TOKEN").expect(
            "set BRAINCRAWL_AUTH_TOKEN or BRAINCRAWL_AUTH_DISABLED=1",
        );
        Arc::new(AuthConfig {
            disabled: false,
            allowlist: SharedSecret::new(&token, "default"),
        })
    };

    let store =
        Arc::new(make_store(&db_path, &blob_root).expect("failed to initialise local store"));
    let app = make_app(store, auth, None);

    let listener = TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind TCP listener");
    eprintln!("braincrawl-server listening on {bind_addr}");
    axum::serve(listener, app).await.expect("server error");
}
