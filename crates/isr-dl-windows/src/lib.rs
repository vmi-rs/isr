//! Download PDB files and PE binaries from Microsoft symbol servers.

mod codeview;
mod error;
mod image_signature;
mod request;

use std::path::{Path, PathBuf};

use bon::Builder;
pub use isr_dl::{Error, ProgressEvent, ProgressFn};
use reqwest::{StatusCode, blocking::Client};
use url::Url;

pub use self::{
    codeview::CodeView, error::DownloaderError, image_signature::ImageSignature,
    request::SymbolRequest,
};

/// Microsoft's public symbol server.
pub const DEFAULT_SERVER_URL: &str = "https://msdl.microsoft.com/download/symbols/";

/// Downloads PDBs and PE binaries from one or more Microsoft symbol servers.
#[derive(Builder)]
pub struct SymbolDownloader {
    #[builder(default)]
    client: Client,
    #[builder(
        default = vec![DEFAULT_SERVER_URL.try_into().unwrap()],
        with = |iter: impl IntoIterator<Item = impl Into<Url>>| {
            iter.into_iter().map(Into::into).collect()
        }
    )]
    servers: Vec<Url>,
    output_directory: PathBuf,
    progress: Option<ProgressFn>,
}

impl SymbolDownloader {
    /// Returns the cached path for `request`, or `None` if it is not on disk.
    pub fn lookup(&self, request: &SymbolRequest) -> Option<PathBuf> {
        let path = self
            .output_directory
            .join(request.subdirectory())
            .join(request.name());

        path.exists().then_some(path)
    }

    /// Returns the on-disk path for `request`, fetching from the configured
    /// servers if the artifact is not cached.
    pub fn download(&self, request: SymbolRequest) -> Result<PathBuf, Error> {
        let output_directory = self.output_directory.join(request.subdirectory());
        std::fs::create_dir_all(&output_directory)?;

        let name = request.name();
        let hash = request.hash();
        let output = output_directory.join(name);

        let mut last_error = None;
        for server in &self.servers {
            let url = match server.join(&format!("{name}/{hash}/{name}")) {
                Ok(url) => url,
                Err(err) => {
                    tracing::debug!(%server, error = %err, "invalid symbol URL, skipping server");
                    last_error = Some(Error::Other(Box::new(DownloaderError::from(err))));
                    continue;
                }
            };

            match self.fetch(&url, &output) {
                Ok(()) => return Ok(output),
                Err(DownloaderError::Http(err)) if err.status() == Some(StatusCode::NOT_FOUND) => {
                    last_error = Some(Error::ArtifactNotFound);
                }
                Err(err) => {
                    tracing::debug!(%server, error = %err, "server error, trying next");
                    last_error = Some(Error::Other(Box::new(err)));
                }
            }
        }

        Err(last_error.unwrap_or(Error::ArtifactNotFound))
    }

    fn fetch(&self, url: &Url, output: &Path) -> Result<(), DownloaderError> {
        if output.exists() {
            tracing::debug!(path = %output.display(), "skipping download");
            return Ok(());
        }

        tracing::debug!(%url, "requesting symbol");
        let mut response = self.client.get(url.clone()).send()?.error_for_status()?;

        let total_bytes = response.content_length();
        isr_dl::stream_download(
            &mut response,
            output,
            url,
            total_bytes,
            self.progress.clone(),
        )?;

        Ok(())
    }
}
