//! File watcher for the L3 root: coalesces bursts of `.l3.md` edits into a
//! single debounced "changed" broadcast, consumed by the `/api/events` SSE route.

use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::broadcast;

/// How long to wait after the last observed change before firing a signal —
/// long enough to coalesce an agent session's multi-file write burst into one.
const DEBOUNCE: Duration = Duration::from_millis(400);

/// Spawn a background thread that watches `root` for `.l3.md` changes and
/// sends `()` on `tx` (debounced) whenever one occurs.
///
/// Runs on its own `std::thread` (not a tokio task) because `notify`'s watcher
/// callback and the debounce wait are both blocking; `broadcast::Sender::send`
/// is synchronous so it can be called directly from that thread.
pub fn spawn_l3_watcher(root: PathBuf, tx: broadcast::Sender<()>) {
    std::thread::spawn(move || {
        let (raw_tx, raw_rx) = std_mpsc::channel::<notify::Result<notify::Event>>();

        let mut watcher = match RecommendedWatcher::new(
            move |res| {
                let _ = raw_tx.send(res);
            },
            notify::Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("l3 watcher: failed to create watcher: {e}");
                return;
            }
        };

        if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
            eprintln!("l3 watcher: failed to watch {}: {e}", root.display());
            return;
        }

        let mut pending = false;
        loop {
            let timeout = if pending {
                DEBOUNCE
            } else {
                Duration::from_secs(3600)
            };
            match raw_rx.recv_timeout(timeout) {
                Ok(Ok(event)) => {
                    if event.paths.iter().any(|p| is_l3_doc(p)) {
                        pending = true;
                    }
                }
                Ok(Err(e)) => eprintln!("l3 watcher: event error: {e}"),
                Err(std_mpsc::RecvTimeoutError::Timeout) => {
                    if pending {
                        pending = false;
                        let _ = tx.send(());
                    }
                }
                Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

fn is_l3_doc(path: &Path) -> bool {
    path.to_str().map(|s| s.ends_with(".l3.md")).unwrap_or(false)
}
