#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Pdb(#[from] pdb::Error),

    #[error("Serialization error: {0}")]
    Serialize(Box<dyn std::error::Error + Send + Sync>),
}
