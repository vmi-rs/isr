//! Request and response types for `UbuntuSymbolDownloader`.

use std::path::PathBuf;

use bon::Builder;

use crate::UbuntuVersionSignature;

/// A request for one or more kernel artifacts.
#[derive(Builder, Debug)]
pub struct UbuntuSymbolRequest {
    /// Version signature identifying the kernel to fetch.
    pub version_signature: UbuntuVersionSignature,

    /// Policy for the kernel image. `None` means do not download.
    pub linux_image: Option<ArtifactPolicy>,

    /// Policy for the kernel debug-symbols image.
    pub linux_image_dbgsym: Option<ArtifactPolicy>,

    /// Policy for the kernel modules package (which contains `System.map`).
    pub linux_modules: Option<ArtifactPolicy>,
}

/// Per-artifact download and optional extraction policy.
#[derive(Builder, Debug, Clone)]
pub struct ArtifactPolicy {
    /// Filename policy for the downloaded `.deb`.
    pub deb: FilenamePolicy,

    /// `None` means keep only the .deb (no extraction).
    pub extract: Option<FilenamePolicy>,
}

/// How to name a file on disk.
#[derive(Debug, Clone)]
pub enum FilenamePolicy {
    /// Use the canonical filename from the package entry (for debs) or the
    /// basename of the path inside the deb (for extracts).
    Original,
    /// Use a caller-supplied filename.
    Custom(PathBuf),
}

impl FilenamePolicy {
    /// Returns a policy for using the original filename.
    pub fn original() -> Self {
        Self::Original
    }

    /// Returns a policy for using a custom filename.
    pub fn custom(path: impl Into<PathBuf>) -> Self {
        Self::Custom(path.into())
    }
}

/// Result of a `download()` call. Mirrors the request structurally: if the
/// request had `Some(policy)` for an artifact, the response has `Some(paths)`.
#[derive(Debug, Default, Clone)]
pub struct UbuntuSymbolPaths {
    /// Directory that holds the per-signature subdirectory.
    pub output_directory: PathBuf,

    /// Paths of the kernel image, if requested.
    pub linux_image: Option<ArtifactPaths>,

    /// Paths of the kernel debug-symbols image, if requested.
    pub linux_image_dbgsym: Option<ArtifactPaths>,

    /// Paths of the kernel modules package, if requested.
    pub linux_modules: Option<ArtifactPaths>,
}

/// Per-artifact resulting paths.
#[derive(Debug, Clone)]
pub struct ArtifactPaths {
    /// Path to the downloaded `.deb`.
    pub deb: PathBuf,

    /// Populated iff the request's `extract` was `Some`.
    pub extracted: Option<PathBuf>,
}
