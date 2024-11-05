#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),

    #[error(transparent)]
    DebError(#[from] debpkg::Error),

    #[error("deb entry not found")]
    DebEntryNotFound,

    #[error("Invalid banner")]
    InvalidBanner,

    #[error("URL does not contain filename")]
    UrlDoesNotContainFilename,

    #[error("Invalid options")]
    InvalidOptions,

    #[error("Missing filename")]
    PackageMissingFilename,

    #[error("Multiple candidates")]
    PackageMultipleCandidates,

    #[error("Package not found")]
    PackageNotFound,
}
