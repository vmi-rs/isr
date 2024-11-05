use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug)]
pub enum LinuxVersionSignature {
    Ubuntu(UbuntuVersionSignature),
}

#[derive(Debug)]
pub struct UbuntuVersionSignature {
    pub release: String,
    pub revision: String,
    pub kernel_flavour: String,
    pub mainline_kernel_version: String,
}

/// Linux banner.
#[derive(Debug)]
pub struct LinuxBanner {
    pub uts_release: String,
    pub linux_compile_by: String,
    pub linux_compile_host: String,
    pub linux_compiler: String,
    pub uts_version: String,
    pub version_signature: Option<LinuxVersionSignature>,
}

// root/debian/rules.d/2-binary-arch.mk (ubuntu CONFIG_VERSION_SIGNATURE)

impl LinuxBanner {
    pub fn parse(banner: &str) -> Option<Self> {
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

        let captures = match LINUX_VERSION_REGEX.captures(banner) {
            Some(captures) => captures,
            None => return None,
        };

        let version_signature = try_parse_ubuntu_signature(&captures["UTS_VERSION"]);

        Some(Self {
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

    let captures = match UBUNTU_VERSION_REGEX.captures(uts_version) {
        Some(captures) => captures,
        None => return None,
    };

    Some(LinuxVersionSignature::Ubuntu(UbuntuVersionSignature {
        release: captures["UBUNTU_RELEASE"].into(),
        revision: captures["UBUNTU_REVISION"].into(),
        kernel_flavour: captures["UBUNTU_KERNEL_FLAVOUR"].into(),
        mainline_kernel_version: captures["UBUNTU_MAINLINE_KERNEL_VERSION"].into(),
    }))
}
