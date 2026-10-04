use indexmap::IndexMap;
use serde::Deserialize;

use crate::{DarwinVersion, DownloaderError};

/// One Kernel Debug Kit listed in the manifest.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KdkManifestEntry {
    /// macOS build identifier, e.g. `26A434`.
    pub build: String,

    /// macOS version, e.g. `27.0.1`.
    pub version: String,

    /// Human-readable KDK name.
    #[serde(default)]
    pub name: Option<String>,

    /// Download URL of the KDK disk image.
    pub url: String,

    /// Size of the disk image in bytes.
    #[serde(default)]
    pub file_size: Option<u64>,

    /// Lowercase hex SHA-256 of the disk image.
    #[serde(default)]
    pub sha256sum: Option<String>,

    /// Lowercase hex MD5 of the disk image.
    #[serde(default)]
    pub md5sum: Option<String>,

    /// Kernel file names mapped to their version strings.
    #[serde(default, rename = "kernel_versions")]
    pub kernel_versions: IndexMap<String, String>,
}

/// List of Kernel Debug Kits published by a KDK manifest.
#[derive(Debug, Clone, Default)]
pub struct KdkManifest {
    /// Manifest entries in document order.
    pub entries: Vec<KdkManifestEntry>,
}

/// Manifest entry matched against a kernel version string.
#[derive(Debug, Clone, Copy)]
pub struct KdkManifestMatch<'a> {
    /// Matching manifest entry.
    pub entry: &'a KdkManifestEntry,

    /// Kernel file name listed for the version string, e.g. `kernel.release.vmapple`.
    pub kernel_name: &'a str,
}

impl KdkManifest {
    /// Parses a manifest from its JSON representation.
    pub fn from_json(json: &[u8]) -> Result<Self, DownloaderError> {
        Ok(Self {
            entries: serde_json::from_slice(json)?,
        })
    }

    /// Finds the entry whose `kernel_versions` lists exactly `version`.
    pub fn resolve(&self, version: &DarwinVersion) -> Option<KdkManifestMatch<'_>> {
        self.entries.iter().find_map(|entry| {
            entry
                .kernel_versions
                .iter()
                .find(|(_, kernel_version)| **kernel_version == version.original)
                .map(|(kernel_name, _)| KdkManifestMatch { entry, kernel_name })
        })
    }

    /// Finds the entry with the given build identifier.
    pub fn find_build(&self, build: &str) -> Option<&KdkManifestEntry> {
        self.entries.iter().find(|entry| entry.build == build)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"[
        {
            "build": "26A434",
            "date": "2026-09-28T00:58:00",
            "fileSize": 1245349997,
            "kernel_versions": {
                "kernel": "Darwin Kernel Version 27.0.0: Tue Aug 11 21:01:59 PDT 2026; root:xnu-13432.1.9~1/RELEASE_X86_64",
                "kernel.release.vmapple": "Darwin Kernel Version 27.0.0: Tue Aug 11 21:00:07 PDT 2026; root:xnu-13432.1.9~1/RELEASE_ARM64_VMAPPLE"
            },
            "md5sum": "4e435fe85b9af8589b90c549ae8b8870",
            "name": "Kernel Debug Kit 27.0.1 build 26A434",
            "sha256sum": "9d0122b21427a78b4659da16d4363ec859bb2058a7a666332191e79a5a6ac96e",
            "url": "https://example.com/26A434/Kernel_Debug_Kit_27.0.1_build_26A434.dmg",
            "version": "27.0.1"
        },
        {
            "build": "22A5266r",
            "url": "https://example.com/22A5266r/Kernel_Debug_Kit.dmg",
            "version": "13.0"
        }
    ]"#;

    #[test]
    fn resolves_exact_version_string() {
        let manifest = KdkManifest::from_json(MANIFEST.as_bytes()).unwrap();
        let version = "Darwin Kernel Version 27.0.0: Tue Aug 11 21:00:07 PDT 2026; \
                       root:xnu-13432.1.9~1/RELEASE_ARM64_VMAPPLE"
            .parse::<DarwinVersion>()
            .unwrap();

        let found = manifest.resolve(&version).unwrap();
        assert_eq!(found.entry.build, "26A434");
        assert_eq!(found.kernel_name, "kernel.release.vmapple");
        assert_eq!(found.entry.file_size, Some(1245349997));
        assert_eq!(
            found.entry.sha256sum.as_deref(),
            Some("9d0122b21427a78b4659da16d4363ec859bb2058a7a666332191e79a5a6ac96e")
        );
        assert!(manifest.find_build("22A5266r").unwrap().sha256sum.is_none());
    }

    #[test]
    fn rejects_inexact_version_string() {
        let manifest = KdkManifest::from_json(MANIFEST.as_bytes()).unwrap();

        // Same kernel, different build timestamp.
        let version = "Darwin Kernel Version 27.0.0: Tue Aug 11 21:00:08 PDT 2026; \
                       root:xnu-13432.1.9~1/RELEASE_ARM64_VMAPPLE"
            .parse::<DarwinVersion>()
            .unwrap();

        assert!(manifest.resolve(&version).is_none());
    }
}
