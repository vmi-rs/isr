mod error;
pub mod repository;
mod repository_cache;

use std::{
    cell::OnceCell,
    fs::File,
    path::{Path, PathBuf},
};

use bon::Builder;
use debpkg::DebPkg;
use reqwest::blocking::Client;
use url::Url;

pub use self::{
    error::Error, repository::UbuntuRepositoryEntry, repository_cache::UbuntuPackageCache,
};
use super::UbuntuVersionSignature;

pub const DEFAULT_DDEBS_URL: &str = "http://ddebs.ubuntu.com";
pub const DEFAULT_ARCHIVE_URL: &str = "http://cz.archive.ubuntu.com/ubuntu";
pub const DEFAULT_ARCH: &str = "amd64";
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
];

enum Filename {
    Original,
    Custom(PathBuf),
}

#[derive(Builder)]
pub struct UbuntuSymbolDownloader {
    #[builder(field)]
    archive_cache: OnceCell<UbuntuPackageCache>,

    #[builder(field)]
    ddebs_cache: OnceCell<UbuntuPackageCache>,

    #[builder(default)]
    client: reqwest::blocking::Client,

    #[builder(default = DEFAULT_ARCH.into())]
    arch: String,

    #[builder(
        default = DEFAULT_DISTS.iter().map(ToString::to_string).collect(),
        with = |iter: impl IntoIterator<Item = impl Into<String>>| {
            iter.into_iter().map(Into::into).collect()
        }
    )]
    dists: Vec<String>,

    #[builder(default = DEFAULT_ARCHIVE_URL.try_into().unwrap())]
    archive_url: Url,
    #[builder(default = DEFAULT_DDEBS_URL.try_into().unwrap())]
    ddebs_url: Url,

    output_directory: PathBuf,
}

#[derive(Debug, Default)]
pub struct UbuntuSymbolPaths {
    pub output_directory: PathBuf,
    pub linux_image_deb: Option<PathBuf>,
    pub linux_image_dbgsym_deb: Option<PathBuf>,
    pub linux_modules_deb: Option<PathBuf>,
    pub linux_image: Option<PathBuf>,
    pub linux_image_dbgsym: Option<PathBuf>,
    pub systemmap: Option<PathBuf>,
}

#[derive(Builder)]
pub struct UbuntuSymbolRequest {
    #[builder(field)]
    linux_image_deb: Option<Filename>,
    #[builder(field)]
    linux_image_dbgsym_deb: Option<Filename>,
    #[builder(field)]
    linux_modules_deb: Option<Filename>,
    #[builder(field)]
    extract_linux_image: Option<Filename>,
    #[builder(field)]
    extract_linux_image_dbgsym: Option<Filename>,
    #[builder(field)]
    extract_systemmap: Option<Filename>,

    version_signature: UbuntuVersionSignature,
    #[builder(default = false)]
    skip_existing: bool,
}

impl<S: ubuntu_symbol_request_builder::State> UbuntuSymbolRequestBuilder<S> {
    pub fn download_linux_image(self) -> Self {
        Self {
            linux_image_deb: Some(Filename::Original),
            ..self
        }
    }

    pub fn download_linux_image_as(self, filename: impl Into<PathBuf>) -> Self {
        Self {
            linux_image_deb: Some(Filename::Custom(filename.into())),
            ..self
        }
    }

    pub fn download_linux_image_dbgsym(self) -> Self {
        Self {
            linux_image_dbgsym_deb: Some(Filename::Original),
            ..self
        }
    }

    pub fn download_linux_image_dbgsym_as(self, filename: impl Into<PathBuf>) -> Self {
        Self {
            linux_image_dbgsym_deb: Some(Filename::Custom(filename.into())),
            ..self
        }
    }

    pub fn download_linux_modules(self) -> Self {
        Self {
            linux_modules_deb: Some(Filename::Original),
            ..self
        }
    }

    pub fn download_linux_modules_as(self, filename: impl Into<PathBuf>) -> Self {
        Self {
            linux_modules_deb: Some(Filename::Custom(filename.into())),
            ..self
        }
    }

    pub fn extract_linux_image(self) -> Self {
        Self {
            extract_linux_image: Some(Filename::Original),
            ..self
        }
    }

    pub fn extract_linux_image_as(self, filename: impl Into<PathBuf>) -> Self {
        Self {
            extract_linux_image: Some(Filename::Custom(filename.into())),
            ..self
        }
    }

    pub fn extract_linux_image_dbgsym(self) -> Self {
        Self {
            extract_linux_image_dbgsym: Some(Filename::Original),
            ..self
        }
    }

    pub fn extract_linux_image_dbgsym_as(self, filename: impl Into<PathBuf>) -> Self {
        Self {
            extract_linux_image_dbgsym: Some(Filename::Custom(filename.into())),
            ..self
        }
    }

    pub fn extract_systemmap(self) -> Self {
        Self {
            extract_systemmap: Some(Filename::Original),
            ..self
        }
    }

    pub fn extract_systemmap_as(self, filename: impl Into<PathBuf>) -> Self {
        Self {
            extract_systemmap: Some(Filename::Custom(filename.into())),
            ..self
        }
    }
}

impl UbuntuSymbolDownloader {
    pub fn download(&self, request: UbuntuSymbolRequest) -> Result<UbuntuSymbolPaths, Error> {
        let subdirectory = request.version_signature.subdirectory();
        let release = request.version_signature.release;
        let revision = request.version_signature.revision;
        let kernel_version = format!("{release}-{revision}");

        //
        // Validate options.
        //

        if request.extract_linux_image.is_some() && request.linux_image_deb.is_none() {
            tracing::error!("extract_linux_image requires download_linux_image");
            return Err(Error::InvalidOptions);
        }

        if request.extract_linux_image_dbgsym.is_some() && request.linux_image_dbgsym_deb.is_none()
        {
            tracing::error!("extract_linux_image_dbgsym requires download_linux_image_dbgsym");
            return Err(Error::InvalidOptions);
        }

        if request.extract_systemmap.is_some() && request.linux_modules_deb.is_none() {
            tracing::error!("extract_systemmap requires download_linux_modules");
            return Err(Error::InvalidOptions);
        }

        let output_directory = self.output_directory.join(subdirectory);
        std::fs::create_dir_all(&output_directory)?;

        let mut result = UbuntuSymbolPaths {
            output_directory: output_directory.clone(),
            ..Default::default()
        };

        if request.linux_image_deb.is_none()
            && request.linux_image_dbgsym_deb.is_none()
            && request.linux_modules_deb.is_none()
        {
            tracing::warn!("no download options specified");
            return Ok(result);
        }

        if request.linux_image_deb.is_some() || request.linux_modules_deb.is_some() {
            // TODO: get_or_try_init
            let packages = match self.archive_cache.get() {
                Some(packages) => packages,
                None => {
                    let packages = UbuntuPackageCache::fetch(
                        &self.client,
                        &self.archive_url,
                        &self.arch,
                        &self.dists,
                    )?;
                    self.archive_cache.get_or_init(|| packages)
                }
            };

            (result.linux_image_deb, result.linux_image) = find_and_download_and_extract(
                &self.client,
                &packages,
                &release,
                &kernel_version,
                &output_directory,
                request.skip_existing,
                find_linux_image_url,
                &format!("./boot/vmlinuz-{release}"),
                request.linux_image_deb.as_ref(),
                request.extract_linux_image.as_ref(),
            )?;

            (result.linux_modules_deb, result.systemmap) = find_and_download_and_extract(
                &self.client,
                &packages,
                &release,
                &kernel_version,
                &output_directory,
                request.skip_existing,
                find_linux_modules_url,
                &format!("./boot/System.map-{release}"),
                request.linux_modules_deb.as_ref(),
                request.extract_systemmap.as_ref(),
            )?;
        }

        if request.linux_image_dbgsym_deb.is_some() {
            // TODO: get_or_try_init
            let packages = match self.ddebs_cache.get() {
                Some(packages) => packages,
                None => {
                    let packages = UbuntuPackageCache::fetch(
                        &self.client,
                        &self.ddebs_url,
                        &self.arch,
                        &self.dists,
                    )?;
                    self.ddebs_cache.get_or_init(|| packages)
                }
            };

            (result.linux_image_dbgsym_deb, result.linux_image_dbgsym) =
                find_and_download_and_extract(
                    &self.client,
                    &packages,
                    &release,
                    &kernel_version,
                    &output_directory,
                    request.skip_existing,
                    find_linux_image_dbgsym_url,
                    &format!("./usr/lib/debug/boot/vmlinux-{release}"),
                    request.linux_image_dbgsym_deb.as_ref(),
                    request.extract_linux_image_dbgsym.as_ref(),
                )?;
        }

        Ok(result)
    }
}

#[expect(clippy::too_many_arguments)]
fn find_and_download_and_extract(
    client: &Client,
    packages: &UbuntuPackageCache,
    release: &str,
    version: &str,
    output_directory: &Path,
    skip_existing: bool,
    find_package_fn: impl Fn(&UbuntuPackageCache, &str, &str) -> Result<Url, Error>,
    deb_entry: &str,
    deb_filename: Option<&Filename>,
    extract_filename: Option<&Filename>,
) -> Result<(Option<PathBuf>, Option<PathBuf>), Error> {
    let deb_filename = match deb_filename {
        Some(deb_filename) => deb_filename,
        None => return Ok((None, None)),
    };

    let url = find_package_fn(packages, release, version)?;
    let deb_path = path_from_url(&url, output_directory, deb_filename)?;

    if deb_path.exists() && skip_existing {
        tracing::debug!(path = %deb_path.display(), "skipping download");
    }
    else {
        download(client, &url, &deb_path)?;
    }

    let extract_filename = match extract_filename {
        Some(extract_filename) => extract_filename,
        None => return Ok((Some(deb_path), None)),
    };

    let path = path_from_deb_entry(deb_entry, output_directory, extract_filename)?;

    if path.exists() && skip_existing {
        tracing::debug!(path = %path.display(), "skipping extraction");
    }
    else {
        unpack_deb_entry(&deb_path, deb_entry, &path)?;
    }

    Ok((Some(deb_path), Some(path)))
}

fn find_linux_image_url(
    packages: &UbuntuPackageCache,
    release: &str,
    version: &str,
) -> Result<Url, Error> {
    let package = format!("linux-image-{release}");
    if let Some(candidate) = packages.find_package(&package, version)? {
        return packages.package_url(candidate);
    }

    let package = format!("linux-image-unsigned-{release}");
    if let Some(candidate) = packages.find_package(&package, version)? {
        return packages.package_url(candidate);
    }

    Err(Error::PackageNotFound)
}

fn find_linux_image_dbgsym_url(
    packages: &UbuntuPackageCache,
    release: &str,
    version: &str,
) -> Result<Url, Error> {
    let package = format!("linux-image-{release}-dbgsym");
    if let Some(candidate) = packages.find_dbgsym_package(&package, version)? {
        return packages.package_url(candidate);
    }

    let package = format!("linux-image-unsigned-{release}-dbgsym");
    if let Some(candidate) = packages.find_dbgsym_package(&package, version)? {
        return packages.package_url(candidate);
    }

    Err(Error::PackageNotFound)
}

fn find_linux_modules_url(
    packages: &UbuntuPackageCache,
    release: &str,
    version: &str,
) -> Result<Url, Error> {
    let package = format!("linux-modules-{release}");
    if let Some(candidate) = packages.find_package(&package, version)? {
        return packages.package_url(candidate);
    }

    Err(Error::PackageNotFound)
}

fn path_from_url(
    url: &Url,
    output_directory: &Path,
    filename: &Filename,
) -> Result<PathBuf, Error> {
    fn extract_file_name_from_url(url: &Url) -> Option<String> {
        url.path_segments()?.next_back().map(ToString::to_string)
    }

    match filename {
        Filename::Original => match extract_file_name_from_url(url) {
            Some(filename) => Ok(output_directory.join(filename)),
            None => {
                tracing::error!("failed to extract filename from URL");
                Err(Error::UrlDoesNotContainFilename)
            }
        },
        Filename::Custom(path) => Ok(output_directory.join(path)),
    }
}

fn download(client: &Client, url: &Url, output_path: impl AsRef<Path>) -> Result<(), Error> {
    let output_path = output_path.as_ref();

    tracing::debug!(%url, "downloading");
    let mut response = client.get(url.clone()).send()?.error_for_status()?;
    let mut file = File::create(output_path)?;
    response.copy_to(&mut file)?;

    Ok(())
}

fn path_from_deb_entry(
    deb_entry_path: impl AsRef<Path>,
    output_directory: impl AsRef<Path>,
    filename: &Filename,
) -> Result<PathBuf, Error> {
    match filename {
        Filename::Original => match deb_entry_path.as_ref().file_name() {
            Some(filename) => Ok(output_directory.as_ref().join(filename)),
            None => {
                tracing::error!("failed to extract filename from deb entry path");
                Err(Error::UrlDoesNotContainFilename)
            }
        },
        Filename::Custom(path) => Ok(output_directory.as_ref().join(path)),
    }
}

fn unpack_deb_entry(
    deb_path: impl AsRef<Path>,
    deb_entry_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> Result<(), Error> {
    let deb_path = deb_path.as_ref();
    let deb_entry_path = deb_entry_path.as_ref();
    let output_path = output_path.as_ref();

    let file = File::open(deb_path)?;
    let mut pkg = DebPkg::parse(file)?;

    let mut data = pkg.data()?;
    for entry in data.entries()? {
        let mut entry = entry?;

        if entry.header().path()? == deb_entry_path {
            tracing::debug!(path = %deb_entry_path.display(), "unpacking");
            entry.unpack(output_path)?;
            return Ok(());
        }
    }

    Err(Error::DebEntryNotFound)
}
