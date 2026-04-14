//! Ubuntu archive downloader.

mod artifacts;
mod error;
mod fetcher;
mod index;
mod parse;
mod repository;
mod request;

use std::{
    cell::OnceCell,
    path::{Path, PathBuf},
    time::Duration,
};

use bon::Builder;
use isr_dl::{Error, ProgressFn};
use reqwest::blocking::Client;
use url::Url;

pub use self::{
    artifacts::{ArtifactRef, KernelArtifacts},
    error::UbuntuError,
    index::{PackageIndex, PackageQuery},
    parse::UbuntuRepositoryEntry,
    request::{
        ArtifactPaths, ArtifactPolicy, FilenamePolicy, UbuntuSymbolPaths, UbuntuSymbolRequest,
    },
};
use self::{fetcher::Fetcher, repository::Repository};
use crate::{DownloaderError, UbuntuVersionSignature};

/// Canonical archive hosting Ubuntu binary `.deb` packages.
pub const DEFAULT_ARCHIVE_URL: &str = "https://archive.ubuntu.com/ubuntu/";

/// Canonical archive hosting Ubuntu detached debug-symbol (`.ddeb`) packages.
pub const DEFAULT_DDEBS_URL: &str = "https://ddebs.ubuntu.com/";

/// Debian architecture string used when none is configured.
pub const DEFAULT_ARCH: &str = "amd64";

/// Ubuntu releases indexed by the downloader when none are configured.
pub const DEFAULT_DISTS: &[&str] = &[
    "trusty",        // 14.04
    "xenial",        // 16.04
    "bionic",        // 18.04
    "focal",         // 20.04
    "focal-updates", // 20.04
    "jammy",         // 22.04
    "jammy-updates", // 22.04
    "noble",         // 24.04
    "noble-updates", // 24.04
    "resolute",      // 26.04
];

/// Downloads Ubuntu kernel + debug symbol `.deb` packages.
#[derive(Builder)]
pub struct UbuntuSymbolDownloader {
    #[builder(field)]
    indices: OnceCell<Vec<PackageIndex>>,

    #[builder(default)]
    client: Client,

    #[builder(into, default = DEFAULT_ARCH)]
    arch: String,

    #[builder(
        default = DEFAULT_DISTS.iter().map(ToString::to_string).collect(),
        with = |iter: impl IntoIterator<Item = impl Into<String>>| {
            iter.into_iter().map(Into::into).collect()
        }
    )]
    dists: Vec<String>,

    #[builder(
        default = vec![
            DEFAULT_ARCHIVE_URL.try_into().unwrap(),
            DEFAULT_DDEBS_URL.try_into().unwrap(),
        ],
        with = |iter: impl IntoIterator<Item = impl Into<Url>>| {
            iter.into_iter().map(Into::into).collect()
        }
    )]
    repository_hosts: Vec<Url>,

    #[builder(into)]
    output_directory: PathBuf,

    progress: Option<ProgressFn>,

    /// Maximum age of cached `Packages.gz` before re-fetching.
    /// `Duration::ZERO` always re-fetches; `Duration::MAX` always uses cache.
    #[builder(default = Duration::from_secs(24 * 3600))]
    index_max_age: Duration,
}

impl UbuntuSymbolDownloader {
    /// Pure filesystem lookup. Returns `Some` only if every requested artifact
    /// (and its extraction, if requested) is already on disk.
    pub fn lookup(&self, request: &UbuntuSymbolRequest) -> Option<UbuntuSymbolPaths> {
        let indices = self.load_cached_indices();
        let artifacts = KernelArtifacts::resolve(&request.version_signature, &indices).ok()?;
        let version_dir = self.version_dir(&request.version_signature);

        Some(UbuntuSymbolPaths {
            output_directory: version_dir.clone(),
            linux_image: lookup_artifact(
                artifacts.linux_image.as_ref(),
                request.linux_image.as_ref(),
                &version_dir,
            )?,
            linux_image_dbgsym: lookup_artifact(
                artifacts.linux_image_dbgsym.as_ref(),
                request.linux_image_dbgsym.as_ref(),
                &version_dir,
            )?,
            linux_modules: lookup_artifact(
                artifacts.linux_modules.as_ref(),
                request.linux_modules.as_ref(),
                &version_dir,
            )?,
        })
    }

    /// Fetches missing artifacts from the network. Always loads (or refreshes)
    /// indices; never short-circuits.
    pub fn download(&self, request: UbuntuSymbolRequest) -> Result<UbuntuSymbolPaths, Error> {
        self.download_inner(request)
            .map_err(|err| Error::Other(Box::new(DownloaderError::Ubuntu(err))))
    }

    fn download_inner(
        &self,
        request: UbuntuSymbolRequest,
    ) -> Result<UbuntuSymbolPaths, UbuntuError> {
        let indices = self.fetch_indices()?;
        let artifacts = KernelArtifacts::resolve(&request.version_signature, indices)?;

        let version_dir = self.version_dir(&request.version_signature);
        std::fs::create_dir_all(&version_dir)?;

        let fetcher = Fetcher::new(&self.client, self.progress.as_ref());

        let linux_image = fetch_artifact(
            &fetcher,
            artifacts.linux_image.as_ref(),
            request.linux_image.as_ref(),
            &version_dir,
        )?;
        let linux_image_dbgsym = fetch_artifact(
            &fetcher,
            artifacts.linux_image_dbgsym.as_ref(),
            request.linux_image_dbgsym.as_ref(),
            &version_dir,
        )?;
        let linux_modules = fetch_artifact(
            &fetcher,
            artifacts.linux_modules.as_ref(),
            request.linux_modules.as_ref(),
            &version_dir,
        )?;

        Ok(UbuntuSymbolPaths {
            output_directory: version_dir,
            linux_image,
            linux_image_dbgsym,
            linux_modules,
        })
    }

    fn fetch_indices(&self) -> Result<&[PackageIndex], UbuntuError> {
        // TODO: get_or_try_init
        let indices = match self.indices.get() {
            Some(indices) => indices,
            None => {
                let mut indices = Vec::with_capacity(self.repository_hosts.len());
                let mut last_error = None;

                for host in &self.repository_hosts {
                    let repo = Repository::new(
                        self.client.clone(),
                        host.clone(),
                        self.arch.clone(),
                        self.dists.clone(),
                    );

                    let index = match repo.fetch_index(
                        &self.index_dir(),
                        self.index_max_age,
                        self.progress.clone(),
                    ) {
                        Ok(index) => index,
                        Err(err) => {
                            tracing::warn!(%err, %host, "failed to fetch index, skipping");
                            last_error = Some(err);
                            continue;
                        }
                    };

                    indices.push(index);
                }

                if indices.is_empty() {
                    return Err(last_error.unwrap_or(UbuntuError::PackageNotFound));
                }

                self.indices.get_or_init(|| indices)
            }
        };

        Ok(indices.as_slice())
    }

    fn load_cached_indices(&self) -> Vec<PackageIndex> {
        let mut indices = Vec::with_capacity(self.repository_hosts.len());
        for host in &self.repository_hosts {
            let repo = Repository::new(
                self.client.clone(),
                host.clone(),
                self.arch.clone(),
                self.dists.clone(),
            );

            let index = match repo.load_cached_index(&self.index_dir(), self.progress.clone()) {
                Ok(index) => index,
                Err(err) => {
                    tracing::debug!(%host, %err, "cached index unavailable, skipping");
                    continue;
                }
            };

            indices.push(index);
        }

        indices
    }

    fn version_dir(&self, signature: &UbuntuVersionSignature) -> PathBuf {
        self.output_directory.join(signature.subdirectory())
    }

    fn index_dir(&self) -> PathBuf {
        self.output_directory.join("_index")
    }
}

/// Resolves the on-disk paths for one artifact according to `policy`. Returns
/// `Some` if `policy` was `Some` and (for `lookup`) all expected files exist.
fn lookup_artifact(
    artifact: Option<&ArtifactRef>,
    policy: Option<&ArtifactPolicy>,
    version_dir: &Path,
) -> Option<Option<ArtifactPaths>> {
    let policy = match policy {
        Some(policy) => policy,
        None => return Some(None),
    };
    let artifact = artifact?;

    let deb_path = resolve_filename(&policy.deb, &artifact.deb_filename, version_dir);
    if !deb_path.exists() {
        return None;
    }

    let extracted = match &policy.extract {
        Some(extracted) => {
            let basename = artifact
                .extract_path
                .file_name()
                .and_then(|filename| filename.to_str())?;
            let path = resolve_filename(extracted, basename, version_dir);

            if !path.exists() {
                return None;
            }

            Some(path)
        }
        None => None,
    };

    Some(Some(ArtifactPaths {
        deb: deb_path,
        extracted,
    }))
}

/// Downloads (and optionally extracts) one artifact according to `policy`.
fn fetch_artifact(
    fetcher: &Fetcher<'_>,
    artifact: Option<&ArtifactRef>,
    policy: Option<&ArtifactPolicy>,
    version_dir: &Path,
) -> Result<Option<ArtifactPaths>, UbuntuError> {
    let policy = match policy {
        Some(policy) => policy,
        None => return Ok(None),
    };
    let artifact = artifact.ok_or(UbuntuError::PackageNotFound)?;

    let deb_path = resolve_filename(&policy.deb, &artifact.deb_filename, version_dir);
    fetcher.fetch_deb(artifact, &deb_path)?;

    let extracted = match &policy.extract {
        Some(extracted) => {
            let basename = artifact
                .extract_path
                .file_name()
                .and_then(|filename| filename.to_str())
                .ok_or(UbuntuError::UrlMissingFilename)?;
            let dest = resolve_filename(extracted, basename, version_dir);
            fetcher.extract_deb_entry(&deb_path, &artifact.extract_path, &dest)?;
            Some(dest)
        }
        None => None,
    };

    Ok(Some(ArtifactPaths {
        deb: deb_path,
        extracted,
    }))
}

/// Joins `version_dir` with either the canonical `original` name or the
/// caller-supplied `Custom` path.
fn resolve_filename(policy: &FilenamePolicy, original: &str, version_dir: &Path) -> PathBuf {
    match policy {
        FilenamePolicy::Original => version_dir.join(original),
        FilenamePolicy::Custom(custom) => version_dir.join(custom),
    }
}
