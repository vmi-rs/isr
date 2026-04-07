#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid banner")]
    InvalidBanner,

    #[error(transparent)]
    UbuntuError(#[from] crate::ubuntu::Error),
}
