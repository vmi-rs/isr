#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    UbuntuError(#[from] crate::ubuntu::Error),
}
