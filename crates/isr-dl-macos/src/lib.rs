//! macOS specific downloaders and utilities.
//!
//! Resolves an XNU kernel version string (e.g. `Darwin Kernel Version 27.0.0:
//! ...; root:xnu-13432.1.9~1/RELEASE_ARM64_VMAPPLE`) to a Kernel Debug Kit
//! through the dortania `KdkSupportPkg` manifest, downloads the KDK disk image
//! and extracts the matching kernel binary and its dSYM. Installed KDK bundles
//! and local disk images can be used instead of a download.

mod cpio;
mod dmg;
mod downloader;
mod error;
mod manifest;
mod version;

pub use isr_dl::{Error, ProgressEvent, ProgressFn};

pub use self::{
    cpio::{KernelCpioSink, KernelFiles},
    downloader::{DEFAULT_MANIFEST_URL, KdkPaths, KdkSymbolDownloader},
    error::DownloaderError,
    manifest::{KdkManifest, KdkManifestEntry, KdkManifestMatch},
    version::DarwinVersion,
};
