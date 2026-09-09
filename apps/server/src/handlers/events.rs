//! GET /api/events — SSE stream of debounced "changed" signals from the L3
//! file watcher. Read-only notification; no body, no write path.

use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;

use crate::AppState;

pub async fn handler_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.changes.subscribe())
        .filter_map(|msg| msg.ok().map(|_| Ok(Event::default().data("changed"))));

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
