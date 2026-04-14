//! HTTP/filesystem layer for fetching and caching Debian package indices.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use flate2::read::GzDecoder;
use indexmap::IndexMap;
use isr_dl::{ProgressFn, ProgressWriter};
use reqwest::blocking::Client;
use url::Url;

use super::{
    error::UbuntuError,
    fetcher::Fetcher,
    index::PackageIndex,
    parse::{UbuntuRepositoryEntry, parse_packages},
};

/// One Debian-style package source: a host, an arch, and a list of dists.
pub struct Repository {
    client: Client,
    host: Url,
    arch: String,
    dists: Vec<String>,
}

impl Repository {
    pub fn new(client: Client, host: Url, arch: String, dists: Vec<String>) -> Self {
        Self {
            client,
            host,
            arch,
            dists,
        }
    }

    /// Loads the index for each dist.
    pub fn fetch_index(
        &self,
        cache_dir: &Path,
        max_age: Duration,
        progress: Option<ProgressFn>,
    ) -> Result<PackageIndex, UbuntuError> {
        let fetcher = Fetcher::new(&self.client, progress.as_ref());
        let mut by_dist = IndexMap::new();

        for dist in &self.dists {
            let path = self.dist_cache_path(cache_dir, dist);

            if !is_fresh(&path, max_age) {
                let url = self.dist_index_url(dist)?;
                fetcher.fetch(&url, &path, true)?;
            }

            let entries = load_dist_from_disk(progress.clone(), &path)?;
            by_dist.insert(dist.clone(), entries_by_package(entries));
        }

        Ok(PackageIndex::new(self.host.clone(), by_dist))
    }

    /// Loads the index purely from cached `Packages.gz` files. Errors if any
    /// required dist's cache is missing. Never touches the network.
    pub fn load_cached_index(
        &self,
        cache_dir: &Path,
        progress: Option<ProgressFn>,
    ) -> Result<PackageIndex, UbuntuError> {
        let mut by_dist = IndexMap::new();

        for dist in &self.dists {
            let path = self.dist_cache_path(cache_dir, dist);
            let entries = load_dist_from_disk(progress.clone(), &path)?;
            by_dist.insert(dist.clone(), entries_by_package(entries));
        }

        Ok(PackageIndex::new(self.host.clone(), by_dist))
    }

    fn dist_cache_path(&self, cache_dir: &Path, dist: &str) -> PathBuf {
        let host_segment = self.host.host_str().unwrap_or("unknown-host");
        cache_dir.join(host_segment).join(dist).join("Packages.gz")
    }

    fn dist_index_url(&self, dist: &str) -> Result<Url, UbuntuError> {
        Ok(self.host.join(&format!(
            "dists/{}/main/binary-{}/Packages.gz",
            dist, self.arch
        ))?)
    }
}

fn load_dist_from_disk(
    progress: Option<ProgressFn>,
    path: &Path,
) -> Result<Vec<UbuntuRepositoryEntry>, UbuntuError> {
    let bytes = std::fs::read(path)?;

    let mut buf = Vec::with_capacity(bytes.len() * 4);
    {
        let mut writer = ProgressWriter::for_extract(progress, &mut buf, path, None);
        let mut decoder = GzDecoder::new(&bytes[..]);
        std::io::copy(&mut decoder, &mut writer)?;
    }

    let text = String::from_utf8(buf).map_err(|_| UbuntuError::PackagesIndexNonUtf8)?;
    Ok(parse_packages(&text))
}

fn entries_by_package(
    entries: Vec<UbuntuRepositoryEntry>,
) -> IndexMap<String, UbuntuRepositoryEntry> {
    let mut map = IndexMap::new();

    for entry in entries {
        if let Some(name) = entry.package.clone() {
            map.entry(name).or_insert(entry);
        }
    }

    map
}

fn is_fresh(path: &Path, max_age: Duration) -> bool {
    let mtime = match std::fs::metadata(path).and_then(|metadata| metadata.modified()) {
        Ok(mtime) => mtime,
        Err(_) => return false,
    };

    SystemTime::now()
        .duration_since(mtime)
        .map(|age| age < max_age)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use super::is_fresh;

    #[test]
    fn missing_file_is_not_fresh() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_fresh(
            &dir.path().join("missing"),
            Duration::from_secs(60)
        ));
    }

    #[test]
    fn just_written_file_is_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("here");
        fs::write(&p, b"x").unwrap();
        assert!(is_fresh(&p, Duration::from_secs(60)));
    }

    #[test]
    fn duration_zero_is_never_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("here");
        fs::write(&p, b"x").unwrap();
        assert!(!is_fresh(&p, Duration::ZERO));
    }

    #[test]
    fn duration_max_is_always_fresh_if_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("here");
        fs::write(&p, b"x").unwrap();
        assert!(is_fresh(&p, Duration::MAX));
    }
}
