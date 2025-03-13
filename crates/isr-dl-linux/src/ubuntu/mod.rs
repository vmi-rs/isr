mod error;
pub mod repository;
mod repository_cache;

use std::{
    fs::File,
    path::{Path, PathBuf},
};

use debpkg::DebPkg;
use url::Url;

pub use self::{
    error::Error, repository::UbuntuRepositoryEntry, repository_cache::UbuntuPackageCache,
};
use crate::{LinuxBanner, LinuxVersionSignature, UbuntuVersionSignature};

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

pub struct UbuntuDownloader {
    arch: String,
    dists: Vec<String>,

    release: String,
    version: String,

    archive_url: Url,
    ddebs_url: Url,

    output_directory: Option<PathBuf>,
    subdirectory: String,
    skip_existing: bool,

    linux_image_deb: Option<Filename>,
    linux_image_dbgsym_deb: Option<Filename>,
    linux_modules_deb: Option<Filename>,
    extract_linux_image: Option<Filename>,
    extract_linux_image_dbgsym: Option<Filename>,
    extract_systemmap: Option<Filename>,
}

#[derive(Debug, Default)]
pub struct UbuntuPaths {
    pub output_directory: PathBuf,
    pub linux_image_deb: Option<PathBuf>,
    pub linux_image_dbgsym_deb: Option<PathBuf>,
    pub linux_modules_deb: Option<PathBuf>,
    pub linux_image: Option<PathBuf>,
    pub linux_image_dbgsym: Option<PathBuf>,
    pub systemmap: Option<PathBuf>,
}

impl UbuntuDownloader {
    pub fn new(release: &str, revision: &str, variant: &str) -> Self {
        //
        // Build the Ubuntu kernel package name and version string.
        // Example:
        //     Ubuntu {
        //         release: "6.8.0",
        //         revision: "40.40~22.04.3",
        //         kernel_flavour: "generic",
        //         mainline_kernel_version: "6.8.12",
        //     }
        //
        // ... results in:
        //     release: "6.8.0-40-generic"
        //     version: "6.8.0-40.40~22.04.3"
        //
        // See https://ubuntu.com/kernel for more information.

        let revision_short = match revision.split_once('.') {
            Some((revision_short, _)) => revision_short,
            None => revision,
        };

        let kernel_release = format!("{release}-{revision_short}-{variant}");
        let kernel_version = format!("{release}-{revision}");
        let subdirectory = format!("{kernel_version}-{variant}");

        Self {
            arch: DEFAULT_ARCH.into(),
            dists: DEFAULT_DISTS.iter().map(ToString::to_string).collect(),
            release: kernel_release,
            version: kernel_version,
            archive_url: DEFAULT_ARCHIVE_URL.try_into().unwrap(),
            ddebs_url: DEFAULT_DDEBS_URL.try_into().unwrap(),
            output_directory: None,
            subdirectory,
            skip_existing: false,
            linux_image_deb: None,
            linux_image_dbgsym_deb: None,
            linux_modules_deb: None,
            extract_linux_image: None,
            extract_linux_image_dbgsym: None,
            extract_systemmap: None,
        }
    }

    pub fn from_banner(banner: &LinuxBanner) -> Result<Self, Error> {
        match &banner.version_signature {
            Some(LinuxVersionSignature::Ubuntu(UbuntuVersionSignature {
                release,
                revision,
                kernel_flavour,
                ..
            })) => Ok(Self::new(release, revision, kernel_flavour)),
            _ => Err(Error::InvalidBanner),
        }
    }

    pub fn destination_path(&self) -> PathBuf {
        match &self.output_directory {
            Some(output_directory) => PathBuf::from(output_directory).join(&self.subdirectory),
            None => PathBuf::from(&self.subdirectory),
        }
    }

    pub fn with_arch(self, arch: impl Into<String>) -> Self {
        Self {
            arch: arch.into(),
            ..self
        }
    }

    pub fn with_dists(self, dists: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            dists: dists.into_iter().map(Into::into).collect(),
            ..self
        }
    }

    pub fn with_archive_url(self, archive_url: Url) -> Self {
        Self {
            archive_url,
            ..self
        }
    }

    pub fn with_ddebs_url(self, ddebs_url: Url) -> Self {
        Self { ddebs_url, ..self }
    }

    pub fn with_output_directory(self, directory: impl Into<PathBuf>) -> Self {
        Self {
            output_directory: Some(directory.into()),
            ..self
        }
    }

    pub fn skip_existing(self) -> Self {
        Self {
            skip_existing: true,
            ..self
        }
    }

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

    pub fn download(self) -> Result<UbuntuPaths, Error> {
        //
        // Validate options.
        //

        if self.extract_linux_image.is_some() && self.linux_image_deb.is_none() {
            tracing::error!("extract_linux_image requires download_linux_image");
            return Err(Error::InvalidOptions);
        }

        if self.extract_linux_image_dbgsym.is_some() && self.linux_image_dbgsym_deb.is_none() {
            tracing::error!("extract_linux_image_dbgsym requires download_linux_image_dbgsym");
            return Err(Error::InvalidOptions);
        }

        if self.extract_systemmap.is_some() && self.linux_modules_deb.is_none() {
            tracing::error!("extract_systemmap requires download_linux_modules");
            return Err(Error::InvalidOptions);
        }

        if self.linux_image_deb.is_none()
            && self.linux_image_dbgsym_deb.is_none()
            && self.linux_modules_deb.is_none()
        {
            tracing::warn!("no download options specified");
            return Err(Error::InvalidOptions);
        }

        let destination_path = self.destination_path();
        std::fs::create_dir_all(&destination_path)?;

        let mut result = UbuntuPaths {
            output_directory: destination_path.clone(),
            ..Default::default()
        };

        if self.linux_image_deb.is_some() || self.linux_modules_deb.is_some() {
            let packages = UbuntuPackageCache::fetch(self.archive_url, &self.arch, &self.dists)?;

            (result.linux_image_deb, result.linux_image) = find_and_download_and_extract(
                &packages,
                &self.release,
                &self.version,
                &destination_path,
                self.skip_existing,
                find_linux_image_url,
                &format!("./boot/vmlinuz-{}", self.release),
                self.linux_image_deb,
                self.extract_linux_image,
            )?;

            (result.linux_modules_deb, result.systemmap) = find_and_download_and_extract(
                &packages,
                &self.release,
                &self.version,
                &destination_path,
                self.skip_existing,
                find_linux_modules_url,
                &format!("./boot/System.map-{}", self.release),
                self.linux_modules_deb,
                self.extract_systemmap,
            )?;
        }

        if self.linux_image_dbgsym_deb.is_some() {
            let packages = UbuntuPackageCache::fetch(self.ddebs_url, &self.arch, &self.dists)?;

            (result.linux_image_dbgsym_deb, result.linux_image_dbgsym) =
                find_and_download_and_extract(
                    &packages,
                    &self.release,
                    &self.version,
                    &destination_path,
                    self.skip_existing,
                    find_linux_image_dbgsym_url,
                    &format!("./usr/lib/debug/boot/vmlinux-{}", self.release),
                    self.linux_image_dbgsym_deb,
                    self.extract_linux_image_dbgsym,
                )?;
        }

        Ok(result)
    }
}

#[expect(clippy::too_many_arguments)]
fn find_and_download_and_extract(
    packages: &UbuntuPackageCache,
    release: &str,
    version: &str,
    output_directory: &Path,
    skip_existing: bool,
    find_package_fn: impl Fn(&UbuntuPackageCache, &str, &str) -> Result<Url, Error>,
    deb_entry: &str,
    deb_filename: Option<Filename>,
    extract_filename: Option<Filename>,
) -> Result<(Option<PathBuf>, Option<PathBuf>), Error> {
    let deb_filename = match deb_filename {
        Some(deb_filename) => deb_filename,
        None => return Ok((None, None)),
    };

    let url = find_package_fn(packages, release, version)?;
    let deb_path = path_from_url(&url, output_directory, deb_filename)?;

    if !deb_path.exists() || !skip_existing {
        download(url, &deb_path)?;
    }
    else {
        tracing::info!(path = %deb_path.display(), "skipping download");
    }

    let extract_filename = match extract_filename {
        Some(extract_filename) => extract_filename,
        None => return Ok((Some(deb_path), None)),
    };

    let path = path_from_deb_entry(deb_entry, output_directory, extract_filename)?;

    if !path.exists() || !skip_existing {
        unpack_deb_entry(&deb_path, deb_entry, &path)?;
    }
    else {
        tracing::info!(path = %path.display(), "skipping extraction");
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
    destination_directory: &Path,
    filename: Filename,
) -> Result<PathBuf, Error> {
    fn extract_file_name_from_url(url: &Url) -> Option<String> {
        url.path_segments()?.next_back().map(ToString::to_string)
    }

    match filename {
        Filename::Original => match extract_file_name_from_url(url) {
            Some(filename) => Ok(destination_directory.join(filename)),
            None => {
                tracing::error!("failed to extract filename from URL");
                Err(Error::UrlDoesNotContainFilename)
            }
        },
        Filename::Custom(path) => Ok(destination_directory.join(path)),
    }
}

fn download(url: Url, destination_path: impl AsRef<Path>) -> Result<(), Error> {
    let destination_path = destination_path.as_ref();

    tracing::info!(%url, "downloading");
    let mut response = reqwest::blocking::get(url)?.error_for_status()?;
    let mut file = File::create(destination_path)?;
    response.copy_to(&mut file)?;

    Ok(())
}

fn path_from_deb_entry(
    deb_entry_path: impl AsRef<Path>,
    destination_directory: &Path,
    filename: Filename,
) -> Result<PathBuf, Error> {
    match filename {
        Filename::Original => match deb_entry_path.as_ref().file_name() {
            Some(filename) => Ok(destination_directory.join(filename)),
            None => {
                tracing::error!("failed to extract filename from deb entry path");
                Err(Error::UrlDoesNotContainFilename)
            }
        },
        Filename::Custom(path) => Ok(destination_directory.join(path)),
    }
}

fn unpack_deb_entry(
    deb_path: impl AsRef<Path>,
    deb_entry_path: impl AsRef<Path>,
    destination_path: impl AsRef<Path>,
) -> Result<(), Error> {
    let deb_path = deb_path.as_ref();
    let deb_entry_path = deb_entry_path.as_ref();
    let destination_path = destination_path.as_ref();

    let file = File::open(deb_path)?;
    let mut pkg = DebPkg::parse(file)?;

    let mut data = pkg.data()?;
    for entry in data.entries()? {
        let mut entry = entry?;

        if entry.header().path()? == deb_entry_path {
            tracing::info!(path = %deb_entry_path.display(), "unpacking");
            entry.unpack(destination_path)?;
            return Ok(());
        }
    }

    Err(Error::DebEntryNotFound)
}
