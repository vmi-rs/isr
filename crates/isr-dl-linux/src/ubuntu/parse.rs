//! Parser for Debian `Packages` files (the per-dist package metadata).

/// One package stanza parsed from a Debian `Packages` file.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct UbuntuRepositoryEntry {
    /// `Package:` field.
    pub package: Option<String>,

    /// `Version:` field.
    pub version: Option<String>,

    /// `Filename:` field, relative to the repository host.
    pub filename: Option<String>,

    /// `Size:` field, the compressed `.deb` size in bytes.
    pub size: Option<usize>,

    /// `Installed-Size:` field, the installed size in KiB.
    pub installed_size: Option<usize>,

    /// `Depends:` field.
    pub depends: Option<String>,

    /// `Section:` field.
    pub section: Option<String>,

    /// `Source:` field.
    pub source: Option<String>,

    /// `MD5sum:` field.
    pub md5sum: Option<String>,

    /// `SHA1:` field.
    pub sha1: Option<String>,

    /// `SHA256:` field.
    pub sha256: Option<String>,

    /// `SHA512:` field.
    pub sha512: Option<String>,
}

/// Parses a decompressed Debian `Packages` file body into a list of entries.
pub fn parse_packages(text: &str) -> Vec<UbuntuRepositoryEntry> {
    let mut result = Vec::new();
    let mut entry = UbuntuRepositoryEntry::default();

    for line in text.lines() {
        if line.is_empty() {
            result.push(entry);
            entry = UbuntuRepositoryEntry::default();
            continue;
        }

        if line.starts_with(' ') {
            continue;
        }

        let (key, value) = match line.split_once(": ") {
            Some(kv) => kv,
            None => continue,
        };

        match key {
            "Package" => entry.package = Some(value.into()),
            "Version" => entry.version = Some(value.into()),
            "Filename" => entry.filename = Some(value.into()),
            "Size" => entry.size = value.parse().ok(),
            "Installed-Size" => entry.installed_size = value.parse().ok(),
            "Depends" => entry.depends = Some(value.into()),
            "Section" => entry.section = Some(value.into()),
            "Source" => entry.source = Some(value.into()),
            "MD5sum" => entry.md5sum = Some(value.into()),
            "SHA1" => entry.sha1 = Some(value.into()),
            "SHA256" => entry.sha256 = Some(value.into()),
            "SHA512" => entry.sha512 = Some(value.into()),
            _ => (),
        }
    }

    if entry.package.is_some() || entry.filename.is_some() {
        result.push(entry);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_entries_with_blank_separator() {
        let input = "\
Package: linux-image-6.8.0-40-generic
Version: 6.8.0-40.40
Filename: pool/main/l/linux/linux-image-6.8.0-40-generic_6.8.0-40.40_amd64.deb
Size: 12345

Package: linux-modules-6.8.0-40-generic
Version: 6.8.0-40.40
Filename: pool/main/l/linux/linux-modules-6.8.0-40-generic_6.8.0-40.40_amd64.deb
Size: 67890
";
        let entries = parse_packages(input);
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].package.as_deref(),
            Some("linux-image-6.8.0-40-generic")
        );
        assert_eq!(entries[0].size, Some(12345));
        assert_eq!(
            entries[1].package.as_deref(),
            Some("linux-modules-6.8.0-40-generic")
        );
    }

    #[test]
    fn ignores_continuation_lines() {
        let input = "\
Package: foo
Description: a package
 with a wrapped description
 across multiple lines
Version: 1.0
";
        let entries = parse_packages(input);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version.as_deref(), Some("1.0"));
    }

    #[test]
    fn skips_lines_without_separator() {
        let input = "Package: foo\nNotAField\nVersion: 1.0\n";
        let entries = parse_packages(input);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version.as_deref(), Some("1.0"));
    }

    #[test]
    fn empty_input_produces_no_entries() {
        assert_eq!(parse_packages("").len(), 0);
    }
}
