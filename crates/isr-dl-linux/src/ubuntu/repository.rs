use std::io::Read as _;

use flate2::read::GzDecoder;
use url::Url;

pub use super::error::Error;

#[derive(Debug, Default)]
pub struct UbuntuRepositoryEntry {
    pub package: Option<String>,
    pub version: Option<String>,
    pub filename: Option<String>,

    pub size: Option<usize>,
    pub installed_size: Option<usize>,

    pub depends: Option<String>,
    pub section: Option<String>,
    pub source: Option<String>,

    pub md5sum: Option<String>,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    pub sha512: Option<String>,
}

pub fn fetch(host: Url, arch: &str, dist: &str) -> Result<Vec<UbuntuRepositoryEntry>, Error> {
    let mut result = Vec::new();
    let full_url = host.join(&format!("dists/{dist}/main/binary-{arch}/Packages.gz"))?;

    tracing::info!(url = %full_url, "requesting");
    let response = reqwest::blocking::get(full_url)?.error_for_status()?;

    let data = response.bytes()?;
    let mut decoder = GzDecoder::new(&data[..]);
    let mut text = String::new();
    decoder.read_to_string(&mut text)?;

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
            Some((key, value)) => (key, value),
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

    Ok(result)
}
