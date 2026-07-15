//! Test fixture helpers for conversion module tests.

use std::path::PathBuf;

/// Return the absolute path to a named fixture file in this directory.
/// The fixture file may not exist yet.
pub fn fixture_plugin(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("conversion")
        .join("test_fixtures")
        .join(name)
}
