//! Resolution of Ubuntu kernel package names + URLs from a version signature.
//!
//! This module is the only place that knows Ubuntu's kernel-naming conventions:
//! - `linux-image-{release}` (with `linux-image-unsigned-` fallback)
//! - `linux-image-{release}-dbgsym` (with `linux-image-unsigned-` fallback)
//! - `linux-modules-{release}`
//!
//! And the deb-internal extraction paths:
//! - `./boot/vmlinuz-{release}`
//! - `./usr/lib/debug/boot/vmlinux-{release}`
//! - `./boot/System.map-{release}`

use std::path::PathBuf;

use url::Url;

use super::{
    error::UbuntuError,
    index::{PackageIndex, PackageQuery},
};
use crate::UbuntuVersionSignature;

/// A reference to a single downloadable artifact (one .deb).
#[derive(Debug, Clone)]
pub struct ArtifactRef {
    /// Full URL of the .deb file on its repository host.
    pub deb_url: Url,

    /// Canonical filename of the .deb (the basename portion of the URL).
    pub deb_filename: String,

    /// Path inside the .deb to the file we care about extracting.
    pub extract_path: PathBuf,
}

/// Resolved kernel artifacts for one version signature.
#[derive(Debug, Default, Clone)]
pub struct KernelArtifacts {
    /// The `linux-image-*` kernel package.
    pub linux_image: Option<ArtifactRef>,

    /// The `linux-image-*-dbgsym` debug symbols package.
    pub linux_image_dbgsym: Option<ArtifactRef>,

    /// The `linux-modules-*` kernel modules package.
    pub linux_modules: Option<ArtifactRef>,
}

impl KernelArtifacts {
    /// Resolves the three kernel artifacts for a version signature against the
    /// provided indices. An artifact is `None` if its package is not found in
    /// any index. Returns an error only on internal lookup failures (e.g.
    /// multiple candidates within one index).
    pub fn resolve(
        version: &UbuntuVersionSignature,
        indices: &[PackageIndex],
    ) -> Result<Self, UbuntuError> {
        let release = version.kernel_release();
        let kernel_version = version.kernel_version();

        Ok(Self {
            linux_image: lookup(
                indices,
                &PackageQuery {
                    package: format!("linux-image-{release}"),
                    version: kernel_version.clone(),
                    dbgsym: false,
                    unsigned_fallback: true,
                },
                PathBuf::from(format!("./boot/vmlinuz-{release}")),
            )?,
            linux_image_dbgsym: lookup(
                indices,
                &PackageQuery {
                    package: format!("linux-image-{release}-dbgsym"),
                    version: kernel_version.clone(),
                    dbgsym: true,
                    unsigned_fallback: true,
                },
                PathBuf::from(format!("./usr/lib/debug/boot/vmlinux-{release}")),
            )?,
            linux_modules: lookup(
                indices,
                &PackageQuery {
                    package: format!("linux-modules-{release}"),
                    version: kernel_version.clone(),
                    dbgsym: false,
                    unsigned_fallback: false,
                },
                PathBuf::from(format!("./boot/System.map-{release}")),
            )?,
        })
    }
}

fn lookup(
    indices: &[PackageIndex],
    query: &PackageQuery,
    extract_path: PathBuf,
) -> Result<Option<ArtifactRef>, UbuntuError> {
    for index in indices {
        if let Some(entry) = index.find(query)? {
            let deb_url = index.resolve_url(entry)?;
            let deb_filename =
                filename_from_url(&deb_url).ok_or(UbuntuError::UrlMissingFilename)?;
            return Ok(Some(ArtifactRef {
                deb_url,
                deb_filename,
                extract_path,
            }));
        }
    }
    Ok(None)
}

fn filename_from_url(url: &Url) -> Option<String> {
    url.path_segments()?.next_back().map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::ubuntu::parse::UbuntuRepositoryEntry;

    fn entry(package: &str, version: &str, filename: &str) -> UbuntuRepositoryEntry {
        UbuntuRepositoryEntry {
            package: Some(package.into()),
            version: Some(version.into()),
            filename: Some(filename.into()),
            ..Default::default()
        }
    }

    fn index_with(host: &str, dist: &str, entries: Vec<UbuntuRepositoryEntry>) -> PackageIndex {
        let mut by_dist = IndexMap::new();
        let mut map = IndexMap::new();
        for entry in entries {
            map.insert(entry.package.clone().unwrap(), entry);
        }
        by_dist.insert(dist.into(), map);
        PackageIndex::new(host.try_into().unwrap(), by_dist)
    }

    fn signature() -> UbuntuVersionSignature {
        UbuntuVersionSignature {
            release: "6.8.0".into(),
            revision: "40.40~22.04.3".into(),
            kernel_flavour: "generic".into(),
            mainline_kernel_version: "6.8.12".into(),
        }
    }

    #[test]
    fn resolves_signed_image() {
        // Package name uses the {release}-{revision_short}-{flavour} form
        // = "6.8.0-40-generic". Version uses {release}-{revision}
        // = "6.8.0-40.40~22.04.3".
        let archive = index_with(
            "http://archive.ubuntu.com/ubuntu/",
            "noble",
            vec![entry(
                "linux-image-6.8.0-40-generic",
                "6.8.0-40.40~22.04.3",
                "pool/main/l/linux/linux-image-6.8.0-40-generic_6.8.0-40.40~22.04.3_amd64.deb",
            )],
        );
        let artifacts = KernelArtifacts::resolve(&signature(), &[archive]).unwrap();
        let img = artifacts.linux_image.expect("linux_image");
        assert_eq!(
            img.deb_filename,
            "linux-image-6.8.0-40-generic_6.8.0-40.40~22.04.3_amd64.deb"
        );
        assert_eq!(
            img.extract_path.to_str().unwrap(),
            "./boot/vmlinuz-6.8.0-40-generic"
        );
    }

    #[test]
    fn resolves_unsigned_when_signed_missing() {
        let archive = index_with(
            "http://archive.ubuntu.com/ubuntu/",
            "noble",
            vec![entry(
                "linux-image-unsigned-6.8.0-40-generic",
                "6.8.0-40.40~22.04.3",
                "pool/main/l/linux/linux-image-unsigned-6.8.0-40-generic_6.8.0-40.40~22.04.3_amd64.deb",
            )],
        );
        let artifacts = KernelArtifacts::resolve(&signature(), &[archive]).unwrap();
        let img = artifacts.linux_image.expect("linux_image");
        assert_eq!(
            img.deb_filename,
            "linux-image-unsigned-6.8.0-40-generic_6.8.0-40.40~22.04.3_amd64.deb"
        );
        assert_eq!(
            img.extract_path.to_str().unwrap(),
            "./boot/vmlinuz-6.8.0-40-generic"
        );
    }

    #[test]
    fn dbgsym_resolves_against_ddebs_when_archive_missing() {
        let archive = index_with("http://archive.ubuntu.com/ubuntu/", "noble", vec![]);
        let ddebs = index_with(
            "http://ddebs.ubuntu.com/",
            "noble",
            vec![entry(
                "linux-image-unsigned-6.8.0-40-generic-dbgsym",
                "6.8.0-40.40~22.04.3",
                "pool/main/l/linux/linux-image-unsigned-6.8.0-40-generic-dbgsym_6.8.0-40.40~22.04.3_amd64.ddeb",
            )],
        );
        let artifacts = KernelArtifacts::resolve(&signature(), &[archive, ddebs]).unwrap();
        let dbgsym = artifacts.linux_image_dbgsym.expect("linux_image_dbgsym");
        assert!(
            dbgsym
                .deb_url
                .as_str()
                .starts_with("http://ddebs.ubuntu.com/")
        );
        assert!(dbgsym.deb_filename.ends_with(".ddeb"));
        assert_eq!(
            dbgsym.extract_path.to_str().unwrap(),
            "./usr/lib/debug/boot/vmlinux-6.8.0-40-generic"
        );
    }

    #[test]
    fn missing_modules_yields_none_not_error() {
        let archive = index_with("http://archive.ubuntu.com/ubuntu/", "noble", vec![]);
        let artifacts = KernelArtifacts::resolve(&signature(), &[archive]).unwrap();
        assert!(artifacts.linux_modules.is_none());
    }
}
