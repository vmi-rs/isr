#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Object(#[from] object::Error),

    #[error("Unsupported architecture {0:?}")]
    UnsupportedArchitecture(object::FileKind),

    #[error("CodeView not found")]
    NotFound,
}
