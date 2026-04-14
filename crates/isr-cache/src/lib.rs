//! # Opinionated cache for OS kernel profiles
//!
//! This crate provides a caching mechanism for profiles generated and used by
//! the [`isr`] crate family. It offers several features to streamline the process
//! of accessing and managing symbol information, including methods for
//! downloading necessary debug symbols for Windows (PDB files) and Linux
//! (DWARF debug info and system map).
//!
//! ## Usage
//!
//! The main component of this crate is the [`IsrCache`] struct.
//!
//! Example of loading a profile from a PDB file using the CodeView information:
//!
//! ```rust,no_run
//! use isr_cache::{CodeView, IsrCache};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a new cache instance.
//! let cache = IsrCache::new("cache")?;
//!
//! // Use the CodeView information of the Windows 10.0.18362.356 kernel.
//! let codeview = CodeView {
//!     name: String::from("ntkrnlmp.pdb"),
//!     guid: String::from("ce7ffb00c20b87500211456b3e905c47"),
//!     age: 1,
//! };
//!
//! // Fetch and create (or get existing) the entry.
//! let entry = cache.entry_from_codeview(codeview)?;
//!
//! // Get the profile from the entry.
//! let profile = entry.profile()?;
//! # Ok(())
//! # }
//! ```
//!
//! Example of loading a profile based on a Linux kernel banner:
//!
//! ```rust,no_run
//! use isr_cache::IsrCache;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a new cache instance.
//! let cache = IsrCache::new("cache")?;
//!
//! // Use the Linux banner of the Ubuntu 6.8.0-40.40~22.04.3-generic kernel.
//! let banner = "Linux version 6.8.0-40-generic \
//!               (buildd@lcy02-amd64-078) \
//!               (x86_64-linux-gnu-gcc-12 (Ubuntu 12.3.0-1ubuntu1~22.04) \
//!               12.3.0, GNU ld (GNU Binutils for Ubuntu) 2.38) \
//!               #40~22.04.3-Ubuntu SMP PREEMPT_DYNAMIC \
//!               Tue Jul 30 17:30:19 UTC 2 \
//!               (Ubuntu 6.8.0-40.40~22.04.3-generic 6.8.12)";
//!
//! // Fetch and create (or get existing) the entry.
//! // Note that the download of Linux debug symbols may take a while.
//! let entry = cache.entry_from_linux_banner(banner)?;
//!
//! // Get the profile from the entry.
//! let profile = entry.profile()?;
//! # Ok(())
//! # }
//! ```
//!
//! Consult the [`vmi`] crate for more information on how to download debug
//! symbols for introspected VMs.
//!
//! [`isr`]: ../isr/index.html
//! [`vmi`]: ../vmi/index.html

mod error;

#[cfg(any(feature = "linux", feature = "windows"))]
use std::cell::OnceCell;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

pub use isr_core::Profile;
pub use isr_dl::{ProgressContext, ProgressEvent, ProgressFn, ProgressWriter};
#[cfg(feature = "linux")]
pub use isr_dl_linux::{
    ArtifactPolicy, FilenamePolicy, LinuxBanner, LinuxVersionSignature, UbuntuSymbolDownloader,
    UbuntuSymbolPaths, UbuntuSymbolRequest, UbuntuVersionSignature,
};
#[cfg(feature = "windows")]
pub use isr_dl_windows::{CodeView, ImageSignature, SymbolDownloader, SymbolRequest};
use memmap2::Mmap;
use rkyv::ser::{Serializer, allocator::Arena, writer::IoWriter};

pub use self::error::Error;

/// File extension of cached rkyv-serialized [`Profile`]s.
pub const PROFILE_FILE_EXTENSION: &str = "isr";

/// An entry in the [`IsrCache`].
pub struct Entry {
    /// The path to the profile.
    profile_path: PathBuf,

    /// The raw profile data.
    data: Mmap,
}

impl Entry {
    /// Creates a new entry from the profile path.
    pub fn new(profile_path: PathBuf) -> Result<Self, Error> {
        let data = unsafe { Mmap::map(&File::open(&profile_path)?)? };
        Ok(Self { profile_path, data })
    }

    /// Returns the path to the profile.
    pub fn profile_path(&self) -> &Path {
        &self.profile_path
    }

    /// Decodes the profile from the entry.
    pub fn profile(&self) -> Result<Profile<'_>, Error> {
        let archived = rkyv::access::<_, rkyv::rancor::Error>(&self.data)?;
        Ok(Profile::from_archived(archived))
    }

    /// Decodes the profile without validating the archived bytes.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that `self.data` is a valid rkyv archive of
    /// [`isr_core::schema::Profile`].
    pub unsafe fn profile_unchecked(&self) -> Result<Profile<'_>, Error> {
        let archived = unsafe { rkyv::access_unchecked(&self.data) };
        Ok(Profile::from_archived(archived))
    }

    /// Deserializes the profile and re-serializes it as JSON.
    #[cfg(feature = "json")]
    pub fn to_json(&self) -> Result<serde_json::Value, Error> {
        let archived =
            rkyv::access::<isr_core::schema::ArchivedProfile, rkyv::rancor::Error>(&self.data)?;
        let deserialized =
            rkyv::deserialize::<isr_core::schema::Profile, rkyv::rancor::Error>(archived)?;

        Ok(serde_json::to_value(&deserialized)?)
    }

    /// Encodes a profile.
    #[allow(unused)]
    fn encode(writer: impl Write, profile: &isr_core::schema::Profile) -> Result<(), Error> {
        let writer = BufWriter::new(writer);
        let mut writer = IoWriter::new(writer);
        let mut arena = Arena::new();

        let mut serializer = Serializer::new(&mut writer, arena.acquire(), ());
        rkyv::api::serialize_using::<_, rkyv::rancor::Error>(profile, &mut serializer)?;

        Ok(())
    }
}

/// A cache for OS kernel profiles.
///
/// Manages the download and extraction of necessary debug symbols.
pub struct IsrCache {
    #[cfg(feature = "linux")]
    ubuntu_downloader: OnceCell<UbuntuSymbolDownloader>,

    #[cfg(feature = "windows")]
    symbol_downloader: OnceCell<SymbolDownloader>,

    /// The directory where cached profiles are stored.
    #[allow(unused)]
    output_directory: PathBuf,

    /// Optional progress callback for download and extraction operations.
    #[allow(unused)]
    progress: Option<ProgressFn>,

    /// If true, the cache will not attempt to download any files and will only
    /// use existing cached profiles.
    #[allow(unused)]
    offline: bool,
}

impl IsrCache {
    /// Creates a new `IsrCache` instance, initializing it with the provided
    /// directory. If the directory doesn't exist, it attempts to create it.
    pub fn new(output_directory: impl Into<PathBuf>) -> Result<Self, Error> {
        let output_directory = output_directory.into();
        std::fs::create_dir_all(&output_directory)?;

        Ok(Self {
            #[cfg(feature = "linux")]
            ubuntu_downloader: OnceCell::new(),
            #[cfg(feature = "windows")]
            symbol_downloader: OnceCell::new(),

            output_directory,
            progress: None,
            offline: false,
        })
    }

    /// Sets a progress callback for download and extraction operations.
    pub fn with_progress(self, f: impl Fn(ProgressEvent<'_>) + Send + Sync + 'static) -> Self {
        Self {
            progress: Some(Arc::new(f)),
            ..self
        }
    }

    /// Enables or disables offline mode.
    ///
    /// In offline mode the cache only uses already-downloaded artifacts and
    /// never reaches out to the network.
    pub fn with_offline(self, offline: bool) -> Self {
        Self { offline, ..self }
    }

    /// Overrides the default [`UbuntuSymbolDownloader`].
    #[cfg(feature = "linux")]
    pub fn with_ubuntu_downloader(self, ubuntu_downloader: UbuntuSymbolDownloader) -> Self {
        Self {
            ubuntu_downloader: OnceCell::from(ubuntu_downloader),
            ..self
        }
    }

    /// Returns the [`UbuntuSymbolDownloader`], lazily initializing it.
    #[cfg(feature = "linux")]
    pub fn ubuntu_downloader(&self) -> &UbuntuSymbolDownloader {
        self.ubuntu_downloader.get_or_init(|| {
            UbuntuSymbolDownloader::builder()
                .output_directory(self.output_directory.join("ubuntu"))
                .maybe_progress(self.progress.clone())
                .build()
        })
    }

    /// Overrides the default [`SymbolDownloader`].
    #[cfg(feature = "windows")]
    pub fn with_symbol_downloader(self, symbol_downloader: SymbolDownloader) -> Self {
        Self {
            symbol_downloader: OnceCell::from(symbol_downloader),
            ..self
        }
    }

    /// Returns the [`SymbolDownloader`], lazily initializing it.
    #[cfg(feature = "windows")]
    pub fn symbol_downloader(&self) -> &SymbolDownloader {
        self.symbol_downloader.get_or_init(|| {
            SymbolDownloader::builder()
                .output_directory(self.output_directory.join("windows"))
                .maybe_progress(self.progress.clone())
                .build()
        })
    }

    /// Creates or retrieves a cached profile based on a Linux kernel banner.
    ///
    /// Parses the banner to determine the kernel version and downloads the
    /// necessary debug symbols and system map if not present in the cache.
    /// Generates and stores the profile, returning its path.
    #[cfg(feature = "linux")]
    pub fn entry_from_linux_banner(&self, linux_banner: &str) -> Result<Entry, Error> {
        let banner = linux_banner
            .parse::<LinuxBanner>()
            .map_err(isr_dl::Error::from)?;

        let output_paths = match banner.version_signature {
            Some(LinuxVersionSignature::Ubuntu(version_signature)) => {
                self.download_from_ubuntu_version_signature(version_signature)?
            }
            _ => {
                // Create a synthetic downloader error.
                return Err(Error::Downloader(isr_dl::Error::Other(Box::new(
                    isr_dl_linux::DownloaderError::InvalidBanner,
                ))));
            }
        };

        let output_directory = output_paths.output_directory;
        let profile_path = output_directory
            .join("profile")
            .with_extension(PROFILE_FILE_EXTENSION);

        with_part_file(profile_path, |profile_file| {
            let kernel_file = File::open(output_directory.join("vmlinux-dbgsym"))?;
            let systemmap_file = File::open(output_directory.join("System.map"))?;
            isr_dwarf::create_profile(kernel_file, systemmap_file, |profile| {
                Entry::encode(profile_file, profile)
            })?;

            Ok(())
        })
    }

    /// Downloads and extracts the kernel image, debug symbols, and
    /// `System.map` for the given Ubuntu version signature.
    ///
    /// Returns an [`UbuntuSymbolPaths`] with the output directory and the
    /// per-artifact paths.
    #[cfg(feature = "linux")]
    pub fn download_from_ubuntu_version_signature(
        &self,
        version_signature: UbuntuVersionSignature,
    ) -> Result<UbuntuSymbolPaths, Error> {
        let request = UbuntuSymbolRequest::builder()
            .version_signature(version_signature)
            .linux_image(
                ArtifactPolicy::builder()
                    // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/linux-image.deb
                    .deb(FilenamePolicy::custom("linux-image.deb"))
                    // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/vmlinuz
                    .extract(FilenamePolicy::custom("vmlinuz"))
                    .build(),
            )
            .linux_image_dbgsym(
                ArtifactPolicy::builder()
                    // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/linux-image-dbgsym.deb
                    .deb(FilenamePolicy::custom("linux-image-dbgsym.deb"))
                    // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/vmlinux-dbgsym
                    .extract(FilenamePolicy::custom("vmlinux-dbgsym"))
                    .build(),
            )
            .linux_modules(
                ArtifactPolicy::builder()
                    // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/linux-modules.deb
                    .deb(FilenamePolicy::custom("linux-modules.deb"))
                    // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/System.map
                    .extract(FilenamePolicy::custom("System.map"))
                    .build(),
            )
            .build();

        if let Some(paths) = self.ubuntu_downloader().lookup(&request) {
            return Ok(paths);
        }

        if self.offline {
            return Err(Error::Downloader(isr_dl::Error::ArtifactNotFound));
        }

        let paths = self.ubuntu_downloader().download(request)?;

        Ok(paths)
    }

    /// Creates or retrieves a cached profile from a [`CodeView`] debug
    /// information structure.
    ///
    /// If a profile for the given `CodeView` information already exists in
    /// the cache, its path is returned. Otherwise, the necessary PDB file is
    /// downloaded, the profile is generated and stored in the cache, and its
    /// path is returned.
    #[cfg(feature = "windows")]
    pub fn entry_from_codeview(&self, codeview: CodeView) -> Result<Entry, Error> {
        // <cache>/windows/ntkrnlmp.pdb/ce7ffb00c20b87500211456b3e905c471
        let output_directory = self
            .output_directory
            .join("windows")
            .join(codeview.subdirectory());

        // <cache>/windows/ntkrnlmp.pdb/ce7ffb00c20b87500211456b3e905c471/ntkrnlmp.pdb
        let pdb_path = self.download_from_codeview(codeview)?;

        // <cache>/windows/ntkrnlmp.pdb/ce7ffb00c20b87500211456b3e905c471/profile.isr
        let profile_path = output_directory
            .join("profile")
            .with_extension(PROFILE_FILE_EXTENSION);

        with_part_file(profile_path, |profile_file| {
            let pdb_file = File::open(&pdb_path)?;
            isr_pdb::create_profile(pdb_file, |profile| Entry::encode(profile_file, profile))?;

            Ok(())
        })
    }

    /// Creates or retrieves a cached profile from a PE file.
    ///
    /// Extracts the [`CodeView`] debug information from the PE file and
    /// delegates to [`entry_from_codeview`].
    ///
    /// [`entry_from_codeview`]: Self::entry_from_codeview
    #[cfg(feature = "windows")]
    pub fn entry_from_pe(&self, path: impl AsRef<Path>) -> Result<Entry, Error> {
        self.entry_from_codeview(CodeView::from_path(path).map_err(isr_dl::Error::from)?)
    }

    /// Downloads or retrieves a cached PDB from its [`CodeView`] record.
    #[cfg(feature = "windows")]
    pub fn download_from_codeview(&self, codeview: CodeView) -> Result<PathBuf, Error> {
        let request = codeview.into();

        if let Some(pdb_path) = self.symbol_downloader().lookup(&request) {
            tracing::debug!(path = %pdb_path.display(), "found cached PE image");
            return Ok(pdb_path);
        }

        if self.offline {
            return Err(Error::Downloader(isr_dl::Error::ArtifactNotFound));
        }

        // <cache>/windows/ntkrnlmp.pdb/ce7ffb00c20b87500211456b3e905c471/ntkrnlmp.pdb
        let pdb_path = self.symbol_downloader().download(request)?;

        Ok(pdb_path)
    }

    /// Downloads or retrieves a cached PE binary from its [`ImageSignature`].
    ///
    /// PE binaries are cached at:
    /// `<cache>/windows/<name>/<timestamp><size_of_image>/<name>`
    ///
    /// Returns the path to the cached binary.
    #[cfg(feature = "windows")]
    pub fn download_from_image_signature(
        &self,
        image_signature: ImageSignature,
    ) -> Result<PathBuf, Error> {
        let request = image_signature.into();

        if let Some(image_path) = self.symbol_downloader().lookup(&request) {
            tracing::debug!(path = %image_path.display(), "found cached PE image");
            return Ok(image_path);
        }

        if self.offline {
            return Err(Error::Downloader(isr_dl::Error::ArtifactNotFound));
        }

        // <cache>/windows/ntoskrnl.exe/7D02613E1047000/ntoskrnl.exe
        let image_path = self.symbol_downloader().download(request)?;

        Ok(image_path)
    }
}

/// Runs `f` against a sibling `.part` file, then renames it over `dest`.
#[allow(unused)]
fn with_part_file<F>(profile_path: PathBuf, f: F) -> Result<Entry, Error>
where
    F: FnOnce(File) -> Result<(), Error>,
{
    if profile_path.exists() {
        tracing::debug!(
            profile_path = %profile_path.display(),
            "profile already exists"
        );

        return Entry::new(profile_path);
    }

    let tmp = profile_path.with_added_extension("part");
    let file = File::create(&tmp)?;
    f(file)?;
    std::fs::rename(&tmp, &profile_path)?;

    Entry::new(profile_path)
}
