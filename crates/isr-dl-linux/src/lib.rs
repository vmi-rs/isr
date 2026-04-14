//! Linux specific downloaders and utilities.

mod banner;
mod error;
pub mod ubuntu;

pub use isr_dl::{Error, ProgressEvent, ProgressFn};

pub use self::{
    banner::{LinuxBanner, LinuxVersionSignature, UbuntuVersionSignature},
    error::DownloaderError,
    ubuntu::{
        ArtifactPaths, ArtifactPolicy, ArtifactRef, FilenamePolicy, KernelArtifacts, PackageIndex,
        PackageQuery, UbuntuRepositoryEntry, UbuntuSymbolDownloader, UbuntuSymbolPaths,
        UbuntuSymbolRequest,
    },
};
