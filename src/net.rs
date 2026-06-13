use std::time::Duration;

use anyhow::{Context, Result};

/// Shared blocking HTTP client with connect and read timeouts.
pub fn blocking_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(300))
        .build()
        .context("Failed to build HTTP client")
}
