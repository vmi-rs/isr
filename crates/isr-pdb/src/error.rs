/// Errors surfaced while building a profile from a PDB file.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// I/O error while reading the PDB file.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Error from the underlying PDB parser.
    #[error(transparent)]
    Pdb(#[from] pdb::Error),

    /// Failed to serialize the generated profile.
    #[error("serialization error: {0}")]
    Serialization(Box<dyn std::error::Error + Send + Sync>),
}
