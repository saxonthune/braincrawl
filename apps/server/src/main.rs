//! Native entry point for braincrawl-server.
//!
//! ## Configuration (env vars)
//!
//! | Variable              | Default          | Description                         |
//! |-----------------------|------------------|-------------------------------------|
//! | `BRAINCRAWL_DB`       | `braincrawl.db`  | Path to the SQLite database file    |
//! | `BRAINCRAWL_BLOB_ROOT`| `blobs`          | Directory for filesystem blob store |
//! | `BRAINCRAWL_BIND`     | `0.0.0.0:8787`   | TCP bind address (default port 8787)|

use std::sync::Arc;

use braincrawl_server_lib::{make_app, make_store};
use tokio::net::TcpListener;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let db_path =
        std::env::var("BRAINCRAWL_DB").unwrap_or_else(|_| "braincrawl.db".to_string());
    let blob_root =
        std::env::var("BRAINCRAWL_BLOB_ROOT").unwrap_or_else(|_| "blobs".to_string());
    let bind_addr =
        std::env::var("BRAINCRAWL_BIND").unwrap_or_else(|_| "0.0.0.0:8787".to_string());

    let store =
        Arc::new(make_store(&db_path, &blob_root).expect("failed to initialise local store"));
    let app = make_app(store);

    let listener = TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind TCP listener");
    eprintln!("braincrawl-server listening on {bind_addr}");
    axum::serve(listener, app).await.expect("server error");
}
