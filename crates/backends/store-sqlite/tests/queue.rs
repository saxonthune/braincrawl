//! Integration tests for `JobEnqueuer` + `JobQueue` on `SqliteStore`.

use braincrawl_core::{
    traits::{JobEnqueuer, JobQueue},
    types::{JobKind, JobSpec},
};
use braincrawl_store_sqlite::SqliteStore;
use tempfile::TempDir;

fn open_store(dir: &TempDir) -> SqliteStore {
    let path = dir.path().join("test.db");
    SqliteStore::open(path.to_str().unwrap()).expect("open db")
}

fn spec(kind: JobKind, target: &str) -> JobSpec {
    JobSpec {
        kind,
        target_id: target.to_string(),
        params: serde_json::json!({}),
    }
}

#[tokio::test]
async fn enqueue_is_idempotent_while_pending() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir);

    let id1 = store.enqueue(spec(JobKind::Fulltext, "doi:10.1/a")).await.unwrap();
    let id2 = store.enqueue(spec(JobKind::Fulltext, "doi:10.1/a")).await.unwrap();

    assert_eq!(id1, id2, "second enqueue must return the same job id");
}

#[tokio::test]
async fn enqueue_different_kinds_are_separate() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir);

    let id_ft = store.enqueue(spec(JobKind::Fulltext, "doi:10.1/a")).await.unwrap();
    let id_refs = store.enqueue(spec(JobKind::Refs, "doi:10.1/a")).await.unwrap();

    assert_ne!(id_ft, id_refs, "different kinds must produce separate jobs");
}

#[tokio::test]
async fn claim_flips_to_running_and_second_claim_is_empty() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir);

    store.enqueue(spec(JobKind::Fulltext, "doi:10.1/b")).await.unwrap();

    let now = "2024-01-01T00:00:01Z";
    let first = store.claim(10, now).await.unwrap();
    assert_eq!(first.len(), 1, "first claim should return the pending job");
    assert_eq!(first[0].kind, JobKind::Fulltext);

    let second = store.claim(10, now).await.unwrap();
    assert!(second.is_empty(), "second claim must not return a running job");
}

#[tokio::test]
async fn complete_moves_state_to_done() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir);

    store.enqueue(spec(JobKind::Refs, "doi:10.1/c")).await.unwrap();
    let now = "2024-01-01T00:00:01Z";
    let jobs = store.claim(1, now).await.unwrap();
    assert_eq!(jobs.len(), 1);

    store.complete(&jobs[0].id).await.unwrap();

    // After complete: re-enqueue should create a NEW job (not coalesce with done).
    let id2 = store.enqueue(spec(JobKind::Refs, "doi:10.1/c")).await.unwrap();
    assert_ne!(id2, jobs[0].id, "completed job should allow a fresh enqueue");
}

#[tokio::test]
async fn retry_requeues_with_incremented_attempts() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir);

    store.enqueue(spec(JobKind::Fulltext, "doi:10.1/d")).await.unwrap();
    let now = "2024-01-01T00:00:01Z";
    let jobs = store.claim(1, now).await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].attempts, 0);

    let run_after = "2099-01-01T00:00:00Z"; // far future so it doesn't get claimed again
    store.retry(&jobs[0].id, run_after, "transient").await.unwrap();

    // Not claimable at `now` (run_after is in the future).
    let reclaimed = store.claim(10, now).await.unwrap();
    assert!(reclaimed.is_empty());

    // Claimable at a future time.
    let future = "2099-01-02T00:00:00Z";
    let reclaimed2 = store.claim(10, future).await.unwrap();
    assert_eq!(reclaimed2.len(), 1);
    assert_eq!(reclaimed2[0].attempts, 1);
}

#[tokio::test]
async fn fail_prevents_reclaim() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir);

    store.enqueue(spec(JobKind::Refs, "doi:10.1/e")).await.unwrap();
    let now = "2024-01-01T00:00:01Z";
    let jobs = store.claim(1, now).await.unwrap();

    store.fail(&jobs[0].id, "permanent error").await.unwrap();

    let reclaimed = store.claim(10, now).await.unwrap();
    assert!(reclaimed.is_empty(), "failed job must not be re-claimed");

    // A new enqueue for the same target should now succeed (no active job remains).
    let id2 = store.enqueue(spec(JobKind::Refs, "doi:10.1/e")).await.unwrap();
    assert_ne!(id2, jobs[0].id);
}
