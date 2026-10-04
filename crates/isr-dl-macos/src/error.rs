use dpp::{DppError, pbzx::PbzxError, xara::XarError};

/// Errors surfaced while resolving macOS Kernel Debug Kits.
#[derive(thiserror::Error, Debug)]
pub enum DownloaderError {
    /// I/O error while reading or writing a cached file.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// HTTP error while fetching the manifest or a Kernel Debug Kit.
    #[error(transparent)]
    Http(#[from] reqwest::Error),

    /// Failed to parse a URL from the manifest.
    #[error(transparent)]
    Url(#[from] url::ParseError),

    /// Failed to parse the KDK manifest.
    #[error(transparent)]
    Manifest(#[from] serde_json::Error),

    /// Failed to read the disk image.
    #[error(transparent)]
    Dmg(#[from] DppError),

    /// Failed to read the installer package.
    #[error(transparent)]
    Pkg(#[from] XarError),

    /// Failed to decompress the package payload.
    #[error(transparent)]
    Payload(#[from] PbzxError),

    /// The supplied kernel version string could not be parsed.
    #[error("invalid Darwin kernel version string")]
    InvalidDarwinVersion,

    /// No manifest entry lists the kernel version string.
    #[error("kernel version not found in the KDK manifest")]
    KernelNotFound,

    /// The KDK build identifier could not be determined.
    #[error("unable to determine the KDK build")]
    UnknownBuild,

    /// The disk image does not contain a Kernel Debug Kit package.
    #[error("Kernel Debug Kit package not found in the disk image")]
    PackageNotFound,

    /// The package payload does not contain the requested kernel or dSYM.
    #[error("kernel `{0}` or its dSYM not found in the package payload")]
    KernelFilesNotFound(String),

    /// The extracted kernel does not embed the requested version string.
    #[error("extracted kernel does not match the requested version")]
    KernelVersionMismatch,

    /// The package payload is not a valid odc cpio archive.
    #[error("invalid cpio archive: {0}")]
    InvalidCpio(&'static str),

    /// The downloaded file does not match the expected SHA-256 digest.
    #[error("SHA-256 mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// Expected digest, lowercase hex.
        expected: String,

        /// Computed digest, lowercase hex.
        actual: String,
    },
}

impl From<DownloaderError> for isr_dl::Error {
    fn from(value: DownloaderError) -> Self {
        isr_dl::Error::Other(Box::new(value))
    }
}
