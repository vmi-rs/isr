/// Error type for the ISR cache.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// An error occurred while serializing or deserializing a profile.
    #[error(transparent)]
    Serialization(#[from] rkyv::rancor::Error),

    /// The profile was generated with a different profile format version.
    #[error("profile format version {found} does not match {expected}")]
    ProfileVersion {
        /// Format version recorded in the profile.
        found: u32,

        /// Format version of this crate, [`isr_core::schema::FORMAT_VERSION`].
        expected: u32,
    },

    /// An error occurred while serializing or deserializing JSON.
    #[cfg(feature = "json")]
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// An error occurred while downloading a symbol file.
    #[cfg(any(feature = "windows", feature = "linux", feature = "macos"))]
    #[error(transparent)]
    Downloader(#[from] isr_dl::Error),

    /// An error occurred while parsing PDB symbols.
    #[cfg(feature = "windows")]
    #[error(transparent)]
    Pdb(#[from] isr_pdb::Error),

    /// An error occurred while parsing DWARF symbols.
    #[cfg(any(feature = "linux", feature = "macos"))]
    #[error(transparent)]
    Dwarf(#[from] isr_dwarf::Error),

    /// The dSYM has no `LC_UUID` load command.
    #[cfg(feature = "macos")]
    #[error("dSYM has no LC_UUID")]
    MissingUuid,

    /// The dSYM `LC_UUID` does not match the expected kernel UUID.
    #[cfg(feature = "macos")]
    #[error("dSYM UUID mismatch: expected {expected:02X?}, got {actual:02X?}")]
    UuidMismatch {
        /// Expected kernel UUID.
        expected: [u8; 16],

        /// `LC_UUID` of the dSYM, if present.
        actual: Option<[u8; 16]>,
    },
}
