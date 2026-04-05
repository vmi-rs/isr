//! Download PDB files and PE binaries from Microsoft symbol servers.

mod codeview;
mod error;
mod pe_info;

use std::{
    fs::File,
    path::{Path, PathBuf},
};

pub use self::{codeview::CodeView, error::Error, pe_info::PeInfo};

pub const DEFAULT_SERVER_URL: &str = "http://msdl.microsoft.com/download/symbols";

pub struct PdbDownloader {
    codeview: CodeView,
    servers: Vec<String>,
    output: Option<PathBuf>,
}

impl PdbDownloader {
    pub fn new(codeview: CodeView) -> Self {
        Self {
            codeview,
            servers: vec![DEFAULT_SERVER_URL.into()],
            output: None,
        }
    }

    pub fn from_exe(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self::new(CodeView::from_path(path)?))
    }

    pub fn with_servers(self, servers: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            servers: servers.into_iter().map(Into::into).collect(),
            ..self
        }
    }

    pub fn with_output(self, output: impl Into<PathBuf>) -> Self {
        Self {
            output: Some(output.into()),
            ..self
        }
    }

    pub fn download(self) -> Result<PathBuf, Error> {
        let CodeView { path, guid } = self.codeview;

        for server in &self.servers {
            let path_with_underscore = path.chars().rev().skip(1).collect::<String>() + "_";

            for suffix in &[&path, &path_with_underscore] {
                let url = format!("{server}/{path}/{guid}/{suffix}");

                tracing::info!(url, "requesting");
                let response = reqwest::blocking::get(&url);
                if response.is_err() {
                    continue;
                }

                let output = match &self.output {
                    Some(output) => {
                        if output.is_dir() {
                            output.join(format!("{guid}_{path}"))
                        }
                        else {
                            output.clone()
                        }
                    }
                    None => PathBuf::from(format!("{guid}_{path}")),
                };

                tracing::info!(?output, "downloading");
                let mut file = File::create(&output)?;
                response?.copy_to(&mut file)?;
                return Ok(output);
            }
        }

        Err(Error::Failed)
    }
}

/// Downloads PE binaries (DLLs/EXEs) from symbol servers.
///
/// PE binaries are indexed by `{TimeDateStamp}{SizeOfImage}`, unlike
/// PDBs which use `{GUID}{age}`.
pub struct PeDownloader {
    pe_info: PeInfo,
    servers: Vec<String>,
    output: Option<PathBuf>,
}

impl PeDownloader {
    pub fn new(pe_info: PeInfo) -> Self {
        Self {
            pe_info,
            servers: vec![DEFAULT_SERVER_URL.into()],
            output: None,
        }
    }

    pub fn with_servers(self, servers: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            servers: servers.into_iter().map(Into::into).collect(),
            ..self
        }
    }

    pub fn with_output(self, output: impl Into<PathBuf>) -> Self {
        Self {
            output: Some(output.into()),
            ..self
        }
    }

    pub fn download(self) -> Result<PathBuf, Error> {
        let PeInfo {
            name,
            timestamp,
            size_of_image,
        } = self.pe_info;

        let index = format!("{timestamp}{size_of_image}");

        for server in &self.servers {
            let url = format!("{server}/{name}/{index}/{name}");

            tracing::info!(url, "requesting PE binary");
            let mut response = match reqwest::blocking::get(&url) {
                Ok(response) if response.status().is_success() => response,
                _ => continue,
            };

            let output = match &self.output {
                Some(output) => {
                    if output.is_dir() {
                        output.join(&name)
                    }
                    else {
                        output.clone()
                    }
                }
                None => PathBuf::from(&name),
            };

            tracing::info!(?output, "downloading PE binary");
            let mut file = File::create(&output)?;
            response.copy_to(&mut file)?;
            return Ok(output);
        }

        Err(Error::Failed)
    }
}
