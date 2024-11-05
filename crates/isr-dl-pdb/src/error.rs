#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    CodeView(#[from] crate::codeview::Error),

    #[error("Failed to download PDB")]
    Failed,
}
