//! Scoped SQLite VM work refusal, using the public query evaluator's progress pattern.
use crate::error::Error;
use axum::http::StatusCode;
use rusqlite::Connection;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
const LIMIT: usize = 32_000_000;
struct Reset<'a>(&'a Connection);
impl Drop for Reset<'_> {
    fn drop(&mut self) {
        let _ = self.0.progress_handler(0, None::<fn() -> bool>);
    }
}
pub(super) fn run<T>(db: &Connection, read: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    let steps = Arc::new(AtomicUsize::new(0));
    let progress = steps.clone();
    db.progress_handler(
        1000,
        Some(move || progress.fetch_add(1000, Ordering::Relaxed) >= LIMIT),
    )?;
    let reset = Reset(db);
    let result = read();
    let exhausted = steps.load(Ordering::Relaxed) >= LIMIT;
    drop(reset);
    if exhausted {
        Err(Error::new(
            StatusCode::BAD_REQUEST,
            "analysis_work_limit",
            "Analysis database work limit reached; choose fewer categories or a shorter date range",
        ))
    } else {
        result
    }
}
