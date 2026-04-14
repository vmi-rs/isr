use std::{path::PathBuf, sync::LazyLock};

use regex::Regex;

use super::DownloaderError;

/// Distribution-specific version signature extracted from a kernel banner.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinuxVersionSignature {
    /// Ubuntu signature.
    Ubuntu(UbuntuVersionSignature),
}

impl LinuxVersionSignature {
    /// Returns the relative subdirectory used to cache artifacts for this signature.
    pub fn subdirectory(&self) -> PathBuf {
        match self {
            Self::Ubuntu(signature) => signature.subdirectory(),
        }
    }
}

/// Ubuntu kernel version signature, as embedded in the UTS version string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UbuntuVersionSignature {
    /// Upstream kernel release, e.g. `6.8.0`.
    pub release: String,

    /// Ubuntu revision, e.g. `40.40~22.04.3`.
    pub revision: String,

    /// Kernel flavour, e.g. `generic`, `lowlatency`.
    pub kernel_flavour: String,

    /// Mainline kernel version the Ubuntu kernel is based on, e.g. `6.8.12`.
    pub mainline_kernel_version: String,
}

// Build the Ubuntu kernel package name and version string.
// Example:
//     UbuntuVersionSignature {
//         release: "6.8.0",
//         revision: "40.40~22.04.3",
//         kernel_flavour: "generic",
//         mainline_kernel_version: "6.8.12",
//     }
//
// ... results in:
//     revision_short   :   "40"
//     kernel_version   :   "6.8.0-40.40~22.04.3"
//     kernel_release   :   "6.8.0-40-generic"
//     subdirectory     :   "6.8.0-40.40~22.04.3-generic"
//
// See https://ubuntu.com/kernel for more information.

impl UbuntuVersionSignature {
    /// Short revision, the portion before the first `.`.
    pub fn revision_short(&self) -> &str {
        match self.revision.split_once('.') {
            Some((revision_short, _)) => revision_short,
            None => &self.revision,
        }
    }

    /// `{release}-{revision_short}-{kernel_flavour}`, e.g. `6.8.0-40-generic`.
    pub fn kernel_release(&self) -> String {
        format!(
            "{}-{}-{}",
            self.release,
            self.revision_short(),
            self.kernel_flavour
        )
    }

    /// `{release}-{revision}`, e.g. `6.8.0-40.40~22.04.3`.
    pub fn kernel_version(&self) -> String {
        format!("{}-{}", self.release, self.revision)
    }

    /// Subdirectory `{kernel_version}-{kernel_flavour}`.
    pub fn subdirectory(&self) -> PathBuf {
        PathBuf::from(format!("{}-{}", self.kernel_version(), self.kernel_flavour))
    }
}

/// Linux banner.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinuxBanner {
    /// `UTS_RELEASE`, e.g. `6.8.0-40-generic`.
    pub uts_release: String,

    /// User part of the compile host, before the `@`.
    pub linux_compile_by: String,

    /// Host part of the compile host, after the `@`.
    pub linux_compile_host: String,

    /// Compiler string, e.g. `x86_64-linux-gnu-gcc-12 ... 12.3.0`.
    pub linux_compiler: String,

    /// `UTS_VERSION`, the portion after `#`.
    pub uts_version: String,

    /// Distribution-specific signature parsed from `uts_version`, if recognized.
    pub version_signature: Option<LinuxVersionSignature>,
}

// root/debian/rules.d/2-binary-arch.mk (ubuntu CONFIG_VERSION_SIGNATURE)

impl std::str::FromStr for LinuxBanner {
    type Err = DownloaderError;

    fn from_str(banner: &str) -> Result<Self, Self::Err> {
        //
        // Linux version 6.8.0-40-generic
        // (buildd@lcy02-amd64-078)
        // (x86_64-linux-gnu-gcc-12 (Ubuntu 12.3.0-1ubuntu1~22.04) 12.3.0, GNU ld (GNU Binutils for Ubuntu) 2.38)
        // #40~22.04.3-Ubuntu SMP PREEMPT_DYNAMIC Tue Jul 30 17:30:19 UTC 2 (Ubuntu 6.8.0-40.40~22.04.3-generic 6.8.12)
        //

        static LINUX_VERSION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(concat!(
                r"Linux version (?<UTS_RELEASE>[0-9]+\.[0-9]+\.[0-9]+[^ ]*) ",
                r"\((?<LINUX_COMPILE_BY>[^@]*)@(?<LINUX_COMPILE_HOST>[^)]*)\) ",
                r"\((?<LINUX_COMPILER>.*)\) ",
                r"#(?<UTS_VERSION>.*)"
            ))
            .unwrap()
        });

        let captures = LINUX_VERSION_REGEX
            .captures(banner)
            .ok_or(DownloaderError::InvalidBanner)?;

        let version_signature = try_parse_ubuntu_signature(&captures["UTS_VERSION"]);

        Ok(Self {
            uts_release: captures["UTS_RELEASE"].to_string(),
            linux_compile_by: captures["LINUX_COMPILE_BY"].to_string(),
            linux_compile_host: captures["LINUX_COMPILE_HOST"].to_string(),
            linux_compiler: captures["LINUX_COMPILER"].to_string(),
            uts_version: captures["UTS_VERSION"].to_string(),
            version_signature,
        })
    }
}

fn try_parse_ubuntu_signature(uts_version: &str) -> Option<LinuxVersionSignature> {
    //
    // (Ubuntu 6.8.0-40.40~22.04.3-generic 6.8.12)
    //

    static UBUNTU_VERSION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(concat!(
            r"\(Ubuntu ",
            r"(?<UBUNTU_RELEASE>.*)-(?<UBUNTU_REVISION>.*)-(?<UBUNTU_KERNEL_FLAVOUR>.*) ",
            r"(?<UBUNTU_MAINLINE_KERNEL_VERSION>.*)\)"
        ))
        .unwrap()
    });

    let captures = UBUNTU_VERSION_REGEX.captures(uts_version)?;

    Some(LinuxVersionSignature::Ubuntu(UbuntuVersionSignature {
        release: captures["UBUNTU_RELEASE"].into(),
        revision: captures["UBUNTU_REVISION"].into(),
        kernel_flavour: captures["UBUNTU_KERNEL_FLAVOUR"].into(),
        mainline_kernel_version: captures["UBUNTU_MAINLINE_KERNEL_VERSION"].into(),
    }))
}
