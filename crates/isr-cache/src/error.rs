/// Error type for the ISR cache.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// An error occurred while serializing or deserializing a profile.
    #[error(transparent)]
    Serialization(#[from] rkyv::rancor::Error),

    /// An error occurred while serializing or deserializing JSON.
    #[cfg(feature = "json")]
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// An error occurred while downloading a symbol file.
    #[cfg(any(feature = "windows", feature = "linux"))]
    #[error(transparent)]
    Downloader(#[from] isr_dl::Error),

    /// An error occurred while parsing PDB symbols.
    #[cfg(feature = "windows")]
    #[error(transparent)]
    Pdb(#[from] isr_pdb::Error),

    /// An error occurred while parsing DWARF symbols.
    #[cfg(feature = "linux")]
    #[error(transparent)]
    Dwarf(#[from] isr_dwarf::Error),
}
