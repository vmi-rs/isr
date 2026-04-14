/// Errors surfaced while fetching and parsing Ubuntu debug symbol packages.
#[derive(thiserror::Error, Debug)]
pub enum UbuntuError {
    /// I/O error while reading or writing a cached file.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// HTTP error while talking to an Ubuntu mirror.
    #[error(transparent)]
    Http(#[from] reqwest::Error),

    /// Failed to parse a URL from the package index.
    #[error(transparent)]
    Url(#[from] url::ParseError),

    /// Failed to parse a Debian `.deb` archive.
    #[error(transparent)]
    DebPackage(#[from] debpkg::Error),

    /// The expected file was not present inside the `.deb` archive.
    #[error("deb entry not found")]
    DebEntryNotFound,

    /// The package URL has no filename component.
    #[error("URL does not contain filename")]
    UrlMissingFilename,

    /// The `Packages` index file contained non-UTF-8 bytes.
    #[error("invalid UTF-8 in Packages index")]
    PackagesIndexNonUtf8,

    /// A package entry in the `Packages` index did not include a `Filename:`
    /// field.
    #[error("missing `Filename` field in package entry")]
    PackageMissingFilename,

    /// More than one package matched the version signature.
    #[error("multiple matching packages")]
    PackageMultipleCandidates,

    /// No package in the index matched the version signature.
    #[error("package not found")]
    PackageNotFound,
}
