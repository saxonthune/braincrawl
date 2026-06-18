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
use braincrawl_core::{
    traits::FetchHandler,
    worker::{tick, BackoffPolicy},
};
use braincrawl_server_lib::{
    handlers::{FulltextHandler, RefsHandler},
    make_app, make_store, AuthConfig,
};
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

    let crossref_mailto = std::env::var("BRAINCRAWL_CROSSREF_MAILTO").ok();
    let unpaywall_email = std::env::var("BRAINCRAWL_UNPAYWALL_EMAIL").ok();

    let handlers: Vec<Box<dyn FetchHandler>> = vec![
        Box::new(FulltextHandler {
            store: Arc::clone(&store),
            unpaywall_email,
        }),
        Box::new(RefsHandler {
            store: Arc::clone(&store),
            crossref_mailto,
        }),
    ];

    let policy = BackoffPolicy {
        max_attempts: 5,
        base_secs: 30,
        factor: 2,
    };

    let app = make_app(Arc::clone(&store), auth);
    let listener = TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind TCP listener");
    eprintln!("braincrawl-server listening on {bind_addr}");

    // Run the worker loop on the local task set alongside the HTTP server.
    // spawn_local avoids the Send requirement on the !Send FetchHandler futures.
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let store_w = Arc::clone(&store);
            tokio::task::spawn_local(async move {
                loop {
                    let n = tick(
                        &store_w.meta,
                        &store_w.coord,
                        &store_w.clock,
                        &handlers,
                        10,
                        &policy,
                    )
                    .await
                    .unwrap_or(0);

                    if n == 0 {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            });

            axum::serve(listener, app).await.expect("server error");
        })
        .await;
}
