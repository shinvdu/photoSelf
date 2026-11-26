//! Core domain logic for photoSelf.
//!
//! The core is deliberately independent from the HTTP layer:
//! config, SQLite storage, scanning, metadata extraction and thumbnails.

pub mod config;
pub mod db;
pub mod model;
pub mod scan;
pub mod thumb;
pub mod util;

pub use config::Config;
pub use db::Db;
