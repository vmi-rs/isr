//! Download PDB files and PE binaries from Microsoft symbol servers.

mod error;
mod request;

use std::{fs::File, path::PathBuf};

use bon::Builder;
use reqwest::blocking::Response;

pub use self::{
    error::Error,
    request::{CodeView, ImageSignature, SymbolKind},
};

pub const DEFAULT_SERVER_URL: &str = "http://msdl.microsoft.com/download/symbols";

#[derive(Builder)]
pub struct SymbolRequest {
    #[builder(start_fn, into)]
    kind: SymbolKind,

    #[builder(default = false)]
    skip_existing: bool,
}

impl SymbolRequest {
    pub fn name(&self) -> &str {
        self.kind.name()
    }

    pub fn hash(&self) -> String {
        self.kind.hash()
    }

    pub fn subdirectory(&self) -> PathBuf {
        self.kind.subdirectory()
    }
}

#[derive(Builder)]
pub struct SymbolDownloader {
    #[builder(default)]
    client: reqwest::blocking::Client,
    #[builder(
        default = vec![DEFAULT_SERVER_URL.into()],
        with = |iter: impl IntoIterator<Item = impl Into<String>>| {
            iter.into_iter().map(Into::into).collect()
        }
    )]
    servers: Vec<String>,
    output_directory: PathBuf,
}

impl SymbolDownloader {
    pub fn download(&self, request: SymbolRequest) -> Result<PathBuf, Error> {
        let output_directory = self.output_directory.join(request.subdirectory());
        std::fs::create_dir_all(&output_directory)?;

        for server in &self.servers {
            if let Ok(path) = self.try_server(server, &request) {
                return Ok(path);
            }
        }

        Err(Error::Failed)
    }

    fn try_server(&self, server: &str, request: &SymbolRequest) -> Result<PathBuf, Error> {
        match self.try_uncompressed(server, request) {
            Ok(path) => Ok(path),
            Err(_) => self.try_compressed(server, request),
        }
    }

    fn try_uncompressed(&self, server: &str, request: &SymbolRequest) -> Result<PathBuf, Error> {
        let name = request.name();
        let hash = request.hash();

        let output = self
            .output_directory
            .join(request.subdirectory())
            .join(name);

        if output.exists() && request.skip_existing {
            tracing::debug!(path = %output.display(), "skipping download");
            return Ok(output);
        }

        let url = format!("{server}/{name}/{hash}/{name}");
        let mut response = self.fetch(&url)?;

        let mut file = File::create(&output)?;
        response.copy_to(&mut file)?;
        Ok(output)
    }

    fn try_compressed(&self, server: &str, request: &SymbolRequest) -> Result<PathBuf, Error> {
        let name = request.name();
        let hash = request.hash();
        let compressed_name = name.chars().rev().skip(1).collect::<String>() + "_";

        let output = self
            .output_directory
            .join(request.subdirectory())
            .join(&compressed_name);

        if output.exists() && request.skip_existing {
            tracing::debug!(path = %output.display(), "skipping download");
            return Ok(output);
        }

        let url = format!("{server}/{name}/{hash}/{compressed_name}");
        let mut response = self.fetch(&url)?;

        let mut file = File::create(&output)?;
        response.copy_to(&mut file)?;
        Ok(output)
    }

    fn fetch(&self, url: &str) -> Result<Response, Error> {
        tracing::debug!(url, "requesting symbol");
        Ok(self.client.get(url).send()?.error_for_status()?)
    }
}
