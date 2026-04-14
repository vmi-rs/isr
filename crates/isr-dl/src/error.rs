/// Errors surfaced by ISR downloaders.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// I/O error while reading or writing a cached file.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// The requested artifact was not found on the server.
    #[error("artifact not found")]
    ArtifactNotFound,

    /// Downloader-specific error not modeled by the variants above.
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}
