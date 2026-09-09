//! `catalog_bytes` reports the real on-disk size of the SQLite file, so it must be
//! non-zero for any opened database — migrations alone allocate pages.

use braincrawl_core::traits::MetadataStore;
use braincrawl_store_sqlite::SqliteStore;

#[tokio::test]
async fn catalog_bytes_reports_on_disk_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("braincrawl.db");
    let store = SqliteStore::open(path.to_str().unwrap()).unwrap();

    let stats = store.stats().await.unwrap();

    assert!(
        stats.catalog_bytes > 0,
        "catalog_bytes was {}, but the migrated database on disk is {} bytes",
        stats.catalog_bytes,
        std::fs::metadata(&path).unwrap().len()
    );
    assert_eq!(
        stats.total_bytes,
        stats.library_bytes + stats.catalog_bytes
    );
}
