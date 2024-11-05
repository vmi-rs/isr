//! Linux specific downloaders and utilities.

mod banner;
mod error;
pub mod ubuntu;

pub use self::{
    banner::{LinuxBanner, LinuxVersionSignature, UbuntuVersionSignature},
    error::Error,
    ubuntu::{UbuntuDownloader, UbuntuPaths},
};
