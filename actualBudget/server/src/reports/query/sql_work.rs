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

#[cfg(test)]
mod tests {
    use super::*;

    // The real production work bound, without an oversized financial fixture or
    // a test-only server setting. Public analysis behavior is covered separately.
    #[test]
    fn exhausted_read_releases_connection_for_later_work() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE saved(value INTEGER); INSERT INTO saved VALUES(42)")
            .unwrap();
        // This finite read exceeds 32 million VM instructions. Its independent
        // result also verifies that recovery can perform equally expensive work.
        let expensive = || -> Result<i64, Error> {
            Ok(db.query_row(
                "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<2000000) SELECT sum(x) FROM n",
                [],
                |r| r.get(0),
            )?)
        };
        let error = run(&db, expensive).unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "analysis_work_limit");
        assert_eq!(expensive().unwrap(), 2_000_001_000_000);
        assert_eq!(
            run(&db, || Ok(db.query_row(
                "SELECT value FROM saved",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
            42
        );
        db.execute("INSERT INTO saved VALUES(7)", []).unwrap();
        assert_eq!(
            db.query_row("SELECT sum(value) FROM saved", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            49
        );
    }
}
