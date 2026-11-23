//! Retained archive/import metadata for retired goal templates. No execution routes.
mod storage;
use crate::error::Error;
use rusqlite::Connection;
use serde_json::{Value, json};
pub use storage::{clock, convert, install, record};
