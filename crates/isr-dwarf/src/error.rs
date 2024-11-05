#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Object(#[from] object::Error),

    #[error(transparent)]
    Gimli(#[from] gimli::Error),

    #[error("invalid system map")]
    InvalidSystemMap,

    #[error("Serialization error: {0}")]
    Serialize(Box<dyn std::error::Error>),
}
