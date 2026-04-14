/// Errors surfaced while resolving Linux kernel debug symbols.
#[derive(thiserror::Error, Debug)]
pub enum DownloaderError {
    /// The supplied kernel banner string could not be parsed.
    #[error("invalid banner")]
    InvalidBanner,

    /// Error originating from the Ubuntu downloader backend.
    #[error(transparent)]
    Ubuntu(#[from] crate::ubuntu::UbuntuError),
}

impl From<DownloaderError> for isr_dl::Error {
    fn from(value: DownloaderError) -> Self {
        isr_dl::Error::Other(Box::new(value))
    }
}
