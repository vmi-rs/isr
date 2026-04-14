//! Per-artifact fetch + extract operations.

use std::{fs::File, path::Path};

use debpkg::DebPkg;
use isr_dl::ProgressFn;
use reqwest::blocking::Client;
use url::Url;

use super::{artifacts::ArtifactRef, error::UbuntuError};

/// Short-lived helper holding shared download infrastructure.
pub struct Fetcher<'a> {
    client: &'a Client,
    progress: Option<&'a ProgressFn>,
}

impl<'a> Fetcher<'a> {
    /// Creates a new `Fetcher`.
    pub fn new(client: &'a Client, progress: Option<&'a ProgressFn>) -> Self {
        Self { client, progress }
    }

    /// Downloads `url` to `output`, emitting download progress. Creates parent
    /// directories as needed. If `output` already exists, re-downloads only
    /// when `overwrite` is true; otherwise returns `Ok(())` without touching
    /// the network.
    pub fn fetch(&self, url: &Url, output: &Path, overwrite: bool) -> Result<(), UbuntuError> {
        if !overwrite && output.exists() {
            tracing::debug!(path = %output.display(), "skipping download");
            return Ok(());
        }

        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }

        tracing::debug!(%url, "downloading");
        let mut response = self.client.get(url.clone()).send()?.error_for_status()?;
        let total_bytes = response.content_length();

        isr_dl::stream_download(
            &mut response,
            output,
            url,
            total_bytes,
            self.progress.cloned(),
        )?;

        Ok(())
    }

    /// Downloads the .deb file for `artifact` to `output`. Skips the network
    /// call if `output` already exists.
    pub fn fetch_deb(&self, artifact: &ArtifactRef, output: &Path) -> Result<(), UbuntuError> {
        self.fetch(&artifact.deb_url, output, false)
    }

    /// Extracts a single entry from a downloaded .deb file. Skips if `output`
    /// already exists.
    pub fn extract_deb_entry(
        &self,
        deb_path: &Path,
        entry_path: &Path,
        output: &Path,
    ) -> Result<(), UbuntuError> {
        if output.exists() {
            tracing::debug!(path = %output.display(), "skipping extraction");
            return Ok(());
        }

        let file = File::open(deb_path)?;
        let mut pkg = DebPkg::parse(file)?;
        let mut data = pkg.data()?;

        for entry in data.entries()? {
            let mut entry = entry?;

            if entry.header().path()? == entry_path {
                tracing::debug!(path = %entry_path.display(), "extracting");
                let total_bytes = entry.header().size().ok();

                if let Some(parent) = output.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                isr_dl::stream_extract(&mut entry, output, total_bytes, self.progress.cloned())?;
                return Ok(());
            }
        }

        Err(UbuntuError::DebEntryNotFound)
    }
}
