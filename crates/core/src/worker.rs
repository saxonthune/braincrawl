//! Worker driver: `tick` runs one batch of fetch jobs from the queue.
//!
//! Core is HTTP-free and platform-agnostic: it defines the algorithm only.
//! The spawned loop and concrete handlers live in `apps/server`.

use crate::{
    traits::{Coordinator, FetchHandler, JobQueue},
    types::DomainError,
};

/// Retry backoff configuration.
pub struct BackoffPolicy {
    pub max_attempts: u32,
    pub base_secs: u64,
    pub factor: u32,
}

/// Compute exponential backoff delay in seconds, capped at 1 hour.
pub fn backoff(attempts: u32, policy: &BackoffPolicy) -> u64 {
    let cap: u64 = 3_600;
    let exp = (policy.factor as u64)
        .saturating_pow(attempts)
        .min(cap / policy.base_secs.max(1));
    policy.base_secs.saturating_mul(exp).min(cap)
}

/// Claim up to `batch` jobs, drive each through its handler, and update the queue.
/// Returns the number of jobs processed (0 ⇒ caller may sleep longer).
pub async fn tick<Q, C, Clk>(
    queue: &Q,
    coord: &C,
    clock: &Clk,
    handlers: &[Box<dyn FetchHandler>],
    batch: u32,
    policy: &BackoffPolicy,
) -> Result<usize, DomainError>
where
    Q: JobQueue,
    C: Coordinator,
    Clk: crate::traits::Clock,
{
    let now = clock.now_rfc3339();
    let jobs = queue.claim(batch, &now).await?;
    let count = jobs.len();

    for job in &jobs {
        let _guard = coord.with_lock(&job.target_id).await?;

        // RateLimiter would gate here

        let handler = handlers.iter().find(|h| h.kind() == job.kind);

        match handler {
            None => {
                queue
                    .fail(&job.id, &format!("no handler for kind {:?}", job.kind))
                    .await?;
            }
            Some(h) => match h.handle(job).await {
                Ok(()) => {
                    queue.complete(&job.id).await?;
                }
                Err(e) => {
                    if job.attempts + 1 >= policy.max_attempts {
                        queue.fail(&job.id, &e.to_string()).await?;
                    } else {
                        let delay = backoff(job.attempts, policy);
                        let run_after = add_secs_to_rfc3339(&now, delay);
                        queue.retry(&job.id, &run_after, &e.to_string()).await?;
                    }
                }
            },
        }
    }

    Ok(count)
}

// ── RFC3339 offset arithmetic (no chrono dep) ─────────────────────────────────

/// Add `secs` to an RFC3339 string of the form `YYYY-MM-DDTHH:MM:SSZ`.
fn add_secs_to_rfc3339(base: &str, secs: u64) -> String {
    let epoch = parse_rfc3339(base).unwrap_or(0).saturating_add(secs);
    secs_to_rfc3339(epoch)
}

fn parse_rfc3339(s: &str) -> Option<u64> {
    if s.len() < 20 {
        return None;
    }
    let year: u64 = s[0..4].parse().ok()?;
    let month: u64 = s[5..7].parse().ok()?;
    let day: u64 = s[8..10].parse().ok()?;
    let h: u64 = s[11..13].parse().ok()?;
    let m: u64 = s[14..16].parse().ok()?;
    let sec: u64 = s[17..19].parse().ok()?;
    let days = ymd_to_days(year, month, day);
    Some(days * 86_400 + h * 3_600 + m * 60 + sec)
}

fn ymd_to_days(year: u64, month: u64, day: u64) -> u64 {
    let mut days: u64 = 0;
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    let months: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    for &dm in months.iter().take((month as usize).saturating_sub(1)) {
        days += dm;
    }
    days + day.saturating_sub(1)
}

fn secs_to_rfc3339(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3_600) % 24;
    let days = secs / 86_400;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        year += 1;
    }
    let months: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &dm in &months {
        if days < dm {
            break;
        }
        days -= dm;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        traits::{Clock, Coordinator, LockGuard},
        types::{DomainError, Job, JobId, JobKind, JobSpec},
    };
    use async_trait::async_trait;
    use std::cell::RefCell;

    // ── Test doubles ──────────────────────────────────────────────────────────

    struct FakeClock;
    impl Clock for FakeClock {
        fn now_rfc3339(&self) -> String {
            "2024-01-01T00:00:00Z".to_string()
        }
    }

    struct FakeCoord;
    #[async_trait(?Send)]
    impl Coordinator for FakeCoord {
        async fn with_lock(&self, _key: &str) -> Result<LockGuard, DomainError> {
            Ok(LockGuard)
        }
    }

    #[derive(Default)]
    struct FakeQueue {
        jobs: RefCell<Vec<Job>>,
        completed: RefCell<Vec<JobId>>,
        retried: RefCell<Vec<(JobId, String, String)>>,
        failed: RefCell<Vec<(JobId, String)>>,
    }

    #[async_trait(?Send)]
    impl JobQueue for FakeQueue {
        async fn claim(&self, limit: u32, _now: &str) -> Result<Vec<Job>, DomainError> {
            let jobs = self.jobs.borrow();
            Ok(jobs.iter().take(limit as usize).cloned().collect())
        }

        async fn complete(&self, id: &JobId) -> Result<(), DomainError> {
            self.completed.borrow_mut().push(id.clone());
            Ok(())
        }

        async fn retry(
            &self,
            id: &JobId,
            run_after: &str,
            err: &str,
        ) -> Result<(), DomainError> {
            self.retried
                .borrow_mut()
                .push((id.clone(), run_after.to_string(), err.to_string()));
            // Increment attempts in the job list
            for job in self.jobs.borrow_mut().iter_mut() {
                if job.id == *id {
                    job.attempts += 1;
                }
            }
            Ok(())
        }

        async fn fail(&self, id: &JobId, err: &str) -> Result<(), DomainError> {
            self.failed
                .borrow_mut()
                .push((id.clone(), err.to_string()));
            Ok(())
        }
    }

    struct SucceedingHandler;
    #[async_trait(?Send)]
    impl FetchHandler for SucceedingHandler {
        fn kind(&self) -> JobKind {
            JobKind::Fulltext
        }
        async fn handle(&self, _job: &Job) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct FailingHandler;
    #[async_trait(?Send)]
    impl FetchHandler for FailingHandler {
        fn kind(&self) -> JobKind {
            JobKind::Refs
        }
        async fn handle(&self, _job: &Job) -> Result<(), DomainError> {
            Err(DomainError::Backend("upstream error".to_string()))
        }
    }

    fn make_job(kind: JobKind, attempts: u32) -> Job {
        Job {
            id: JobId("job-1".to_string()),
            kind,
            target_id: "doi:10.1/test".to_string(),
            params: serde_json::Value::Null,
            attempts,
        }
    }

    fn policy(max_attempts: u32) -> BackoffPolicy {
        BackoffPolicy {
            max_attempts,
            base_secs: 30,
            factor: 2,
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn succeeding_handler_completes() {
        let q = FakeQueue::default();
        q.jobs.borrow_mut().push(make_job(JobKind::Fulltext, 0));
        let handlers: Vec<Box<dyn FetchHandler>> = vec![Box::new(SucceedingHandler)];

        let n = tick(&q, &FakeCoord, &FakeClock, &handlers, 10, &policy(3))
            .await
            .unwrap();

        assert_eq!(n, 1);
        assert_eq!(q.completed.borrow().len(), 1);
        assert!(q.retried.borrow().is_empty());
        assert!(q.failed.borrow().is_empty());
    }

    #[tokio::test]
    async fn failing_handler_retries_then_fails() {
        let q = FakeQueue::default();
        q.jobs.borrow_mut().push(make_job(JobKind::Refs, 0));
        let handlers: Vec<Box<dyn FetchHandler>> = vec![Box::new(FailingHandler)];
        let p = policy(3);

        // tick 1: attempts=0 → retry (0+1 < 3)
        tick(&q, &FakeCoord, &FakeClock, &handlers, 10, &p).await.unwrap();
        assert!(q.completed.borrow().is_empty());
        assert_eq!(q.retried.borrow().len(), 1);
        assert!(q.failed.borrow().is_empty());

        // tick 2: attempts=1 → retry (1+1 < 3)
        tick(&q, &FakeCoord, &FakeClock, &handlers, 10, &p).await.unwrap();
        assert_eq!(q.retried.borrow().len(), 2);
        assert!(q.failed.borrow().is_empty());

        // tick 3: attempts=2 → fail (2+1 >= 3)
        tick(&q, &FakeCoord, &FakeClock, &handlers, 10, &p).await.unwrap();
        assert_eq!(q.failed.borrow().len(), 1);
        assert_eq!(q.retried.borrow().len(), 2);
    }

    #[tokio::test]
    async fn no_handler_for_kind_fails_job() {
        let q = FakeQueue::default();
        q.jobs.borrow_mut().push(make_job(JobKind::Refs, 0));
        // Only a Fulltext handler — no Refs handler
        let handlers: Vec<Box<dyn FetchHandler>> = vec![Box::new(SucceedingHandler)];

        tick(&q, &FakeCoord, &FakeClock, &handlers, 10, &policy(3))
            .await
            .unwrap();

        assert!(q.completed.borrow().is_empty());
        assert!(q.retried.borrow().is_empty());
        assert_eq!(q.failed.borrow().len(), 1);
    }

    #[test]
    fn backoff_values_are_capped() {
        let p = BackoffPolicy { max_attempts: 5, base_secs: 30, factor: 2 };
        assert_eq!(backoff(0, &p), 30);
        assert_eq!(backoff(1, &p), 60);
        assert_eq!(backoff(2, &p), 120);
        assert!(backoff(100, &p) <= 3_600);
    }

    #[test]
    fn rfc3339_roundtrip() {
        let base = "2024-06-01T12:00:00Z";
        let result = add_secs_to_rfc3339(base, 3600);
        assert_eq!(result, "2024-06-01T13:00:00Z");
    }

    #[test]
    fn rfc3339_add_crosses_day() {
        let base = "2024-06-01T23:30:00Z";
        let result = add_secs_to_rfc3339(base, 3600);
        assert_eq!(result, "2024-06-02T00:30:00Z");
    }

    // Trivial enqueue smoke test (using just type construction)
    #[test]
    fn job_spec_roundtrip() {
        let spec = JobSpec {
            kind: JobKind::Fulltext,
            target_id: "doi:10.1/test".to_string(),
            params: serde_json::json!({}),
        };
        assert_eq!(spec.kind.as_str(), "fulltext");
    }
}
