//! Download PDB files from Microsoft symbol servers.

mod codeview;
mod error;

use std::{
    fs::File,
    path::{Path, PathBuf},
};

pub use self::{codeview::CodeView, error::Error};

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
