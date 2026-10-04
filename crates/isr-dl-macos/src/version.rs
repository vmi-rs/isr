//! XNU kernel version string parsing.

use std::str::FromStr;

use crate::DownloaderError;

/// Prefix of every XNU kernel version string.
const PREFIX: &str = "Darwin Kernel Version ";

/// Parsed XNU kernel version string.
///
/// The string has the form
/// `Darwin Kernel Version <release>: <date>; root:<project>-<xnu>~<build>/<CONFIG>_<ARCH>[_<PLATFORM>]`,
/// e.g. `Darwin Kernel Version 27.0.0: Tue Aug 11 21:00:07 PDT 2026; root:xnu-13432.1.9~1/RELEASE_ARM64_VMAPPLE`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DarwinVersion {
    /// Original version string, used for exact manifest matching.
    pub original: String,

    /// Darwin release, e.g. `27.0.0`.
    pub release: String,

    /// Build date, e.g. `Tue Aug 11 21:00:07 PDT 2026`.
    pub date: String,

    /// Build project, e.g. `xnu` or `xnu_development`.
    pub project: String,

    /// XNU source version, e.g. `13432.1.9`.
    pub xnu_version: String,

    /// Build number following the `~`, e.g. `1`.
    pub build_number: String,

    /// Kernel configuration, e.g. `RELEASE`, `DEVELOPMENT` or `KASAN`.
    pub config: String,

    /// CPU architecture, e.g. `ARM64` or `X86_64`.
    pub arch: String,

    /// Hardware platform for ARM64 kernels, e.g. `VMAPPLE` or `T8103`.
    pub platform: Option<String>,
}

impl DarwinVersion {
    /// Returns the kernel file name inside a Kernel Debug Kit.
    ///
    /// X86_64 release kernels are named `kernel`, other X86_64 kernels
    /// `kernel.<config>`, ARM64 kernels `kernel.<config>.<platform>`, all in
    /// lowercase (e.g. `kernel.release.vmapple`).
    pub fn kernel_name(&self) -> String {
        let config = self.config.to_ascii_lowercase();

        match &self.platform {
            Some(platform) => format!("kernel.{config}.{}", platform.to_ascii_lowercase()),
            None if config == "release" => String::from("kernel"),
            None => format!("kernel.{config}"),
        }
    }
}

impl FromStr for DarwinVersion {
    type Err = DownloaderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let original = s.trim_end_matches(['\0', '\n', '\r', ' ']);
        parse(original).ok_or(DownloaderError::InvalidDarwinVersion)
    }
}

/// Parses `original`, returning `None` on any malformed component.
fn parse(original: &str) -> Option<DarwinVersion> {
    let rest = original.strip_prefix(PREFIX)?;
    let (release, rest) = rest.split_once(": ")?;
    let (date, rest) = rest.split_once("; root:")?;
    let (project_version, kernel_config) = rest.split_once('/')?;
    let (project_xnu, build_number) = project_version.split_once('~')?;
    let (project, xnu_version) = project_xnu.split_once('-')?;
    let (config, arch_platform) = kernel_config.split_once('_')?;

    let (arch, platform) = match arch_platform.strip_prefix("X86_64") {
        Some("") => ("X86_64", None),
        Some(platform) => ("X86_64", Some(platform.strip_prefix('_')?)),
        None => match arch_platform.split_once('_') {
            Some((arch, platform)) => (arch, Some(platform)),
            None => (arch_platform, None),
        },
    };

    let fields = [
        release,
        date,
        project,
        xnu_version,
        build_number,
        config,
        arch,
    ];

    if fields.iter().any(|field| field.is_empty()) || platform == Some("") {
        return None;
    }

    Some(DarwinVersion {
        original: original.to_owned(),
        release: release.to_owned(),
        date: date.to_owned(),
        project: project.to_owned(),
        xnu_version: xnu_version.to_owned(),
        build_number: build_number.to_owned(),
        config: config.to_owned(),
        arch: arch.to_owned(),
        platform: platform.map(ToOwned::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel_name(version: &str) -> String {
        version.parse::<DarwinVersion>().unwrap().kernel_name()
    }

    #[test]
    fn parses_arm64_vmapple() {
        let version = "Darwin Kernel Version 27.0.0: Tue Aug 11 21:00:07 PDT 2026; \
                       root:xnu-13432.1.9~1/RELEASE_ARM64_VMAPPLE"
            .parse::<DarwinVersion>()
            .unwrap();

        assert_eq!(version.release, "27.0.0");
        assert_eq!(version.date, "Tue Aug 11 21:00:07 PDT 2026");
        assert_eq!(version.project, "xnu");
        assert_eq!(version.xnu_version, "13432.1.9");
        assert_eq!(version.build_number, "1");
        assert_eq!(version.config, "RELEASE");
        assert_eq!(version.arch, "ARM64");
        assert_eq!(version.platform.as_deref(), Some("VMAPPLE"));
        assert_eq!(version.kernel_name(), "kernel.release.vmapple");
    }

    #[test]
    fn kernel_names() {
        assert_eq!(
            kernel_name(
                "Darwin Kernel Version 27.0.0: Tue Aug 11 21:01:59 PDT 2026; \
                 root:xnu-13432.1.9~1/RELEASE_X86_64"
            ),
            "kernel"
        );
        assert_eq!(
            kernel_name(
                "Darwin Kernel Version 27.0.0: Tue Aug 11 20:56:26 PDT 2026; \
                 root:xnu_development-13432.1.9~1/DEVELOPMENT_X86_64"
            ),
            "kernel.development"
        );
        assert_eq!(
            kernel_name(
                "Darwin Kernel Version 27.0.0: Tue Aug 11 21:04:41 PDT 2026; \
                 root:xnu_kasan-13432.1.9~1/KASAN_X86_64"
            ),
            "kernel.kasan"
        );
        assert_eq!(
            kernel_name(
                "Darwin Kernel Version 27.0.0: Tue Aug 11 21:05:27 PDT 2026; \
                 root:xnu-13432.1.9~1/RELEASE_ARM64_T8103"
            ),
            "kernel.release.t8103"
        );
        assert_eq!(
            kernel_name(
                "Darwin Kernel Version 22.0.0: Tue May 24 20:28:50 PDT 2022; \
                 root:xnu-8792.0.50.111.3~5/DEVELOPMENT_ARM64_T8103"
            ),
            "kernel.development.t8103"
        );
    }

    #[test]
    fn keeps_original_without_trailing_nul() {
        let version = "Darwin Kernel Version 27.0.0: Tue Aug 11 21:00:07 PDT 2026; \
                       root:xnu-13432.1.9~1/RELEASE_ARM64_VMAPPLE\0\0"
            .parse::<DarwinVersion>()
            .unwrap();

        assert_eq!(
            version.original,
            "Darwin Kernel Version 27.0.0: Tue Aug 11 21:00:07 PDT 2026; \
             root:xnu-13432.1.9~1/RELEASE_ARM64_VMAPPLE"
        );
    }

    #[test]
    fn rejects_malformed() {
        for version in [
            "",
            "Linux version 6.8.0-40-generic",
            "Darwin Kernel Version 27.0.0: Tue Aug 11 21:00:07 PDT 2026",
            "Darwin Kernel Version 27.0.0: date; root:xnu-13432.1.9/RELEASE_ARM64_VMAPPLE",
            "Darwin Kernel Version 27.0.0: date; root:xnu-13432.1.9~1/RELEASE",
            "Darwin Kernel Version 27.0.0: date; root:xnu-13432.1.9~1/RELEASE_ARM64_",
        ] {
            assert!(version.parse::<DarwinVersion>().is_err(), "{version}");
        }
    }
}
