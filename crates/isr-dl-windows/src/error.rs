/// Errors returned by the Windows symbol downloader.
#[derive(thiserror::Error, Debug)]
pub enum DownloaderError {
    /// Filesystem I/O failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// The HTTP request to a symbol server failed.
    #[error(transparent)]
    Http(#[from] reqwest::Error),

    /// Construction of a symbol URL failed.
    #[error(transparent)]
    Url(#[from] url::ParseError),

    /// Parsing a PE file failed.
    #[error(transparent)]
    Object(#[from] object::Error),

    /// The file is not a supported PE kind (neither PE32 nor PE32+).
    #[error("unsupported file kind: {0:?}")]
    UnsupportedFileKind(object::FileKind),

    /// The PE file has no CodeView debug directory, so there is no PDB to
    /// download.
    #[error("CodeView info not found")]
    MissingCodeView,
}

impl From<DownloaderError> for isr_dl::Error {
    fn from(value: DownloaderError) -> Self {
        isr_dl::Error::Other(Box::new(value))
    }
}
