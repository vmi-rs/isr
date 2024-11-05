/// Error type for the ISR cache.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// An error occurred while parsing PDB symbols.
    #[cfg(feature = "pdb")]
    #[error(transparent)]
    Pdb(#[from] isr_pdb::Error),

    /// An error occurred while parsing DWARF symbols.
    #[cfg(feature = "linux")]
    #[error(transparent)]
    Dwarf(#[from] isr_dwarf::Error),

    /// An error occurred while downloading a PDB file.
    #[cfg(feature = "pdb")]
    #[error(transparent)]
    PdbDownloader(#[from] isr_dl_pdb::Error),

    /// An error occurred while downloading Linux symbols.
    #[cfg(feature = "linux")]
    #[error(transparent)]
    LinuxDownloader(#[from] isr_dl_linux::Error),

    /// An error occurred while parsing a Linux kernel banner.
    #[cfg(feature = "linux")]
    #[error("Invalid banner")]
    InvalidBanner,
}
