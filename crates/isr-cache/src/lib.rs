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
//! ```rust
//! use isr::{
//!     download::pdb::CodeView,
//!     cache::{IsrCache, JsonCodec},
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # std::env::set_current_dir("../..")?;
//! // Create a new cache instance.
//! let cache = IsrCache::<JsonCodec>::new("cache")?;
//!
//! // Use the CodeView information of the Windows 10.0.18362.356 kernel.
//! let codeview = CodeView {
//!     path: String::from("ntkrnlmp.pdb"),
//!     guid: String::from("ce7ffb00c20b87500211456b3e905c471"),
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
//! ```rust
//! use isr::cache::{IsrCache, JsonCodec};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # std::env::set_current_dir("../..")?;
//! // Create a new cache instance.
//! let cache = IsrCache::<JsonCodec>::new("cache")?;
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

mod codec;
mod error;

#[cfg(feature = "pdb")]
use std::cell::OnceCell;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

pub use isr_core::Profile;
#[cfg(feature = "linux")]
pub use isr_dl_linux::{
    LinuxBanner, LinuxVersionSignature, UbuntuSymbolDownloader, UbuntuSymbolPaths,
    UbuntuSymbolRequest, UbuntuVersionSignature,
};
#[cfg(feature = "pdb")]
pub use isr_dl_pdb::{CodeView, ImageSignature, SymbolDownloader, SymbolKind, SymbolRequest};
use memmap2::Mmap;

#[cfg(feature = "codec-bincode")]
pub use self::codec::BincodeCodec;
#[cfg(feature = "codec-json")]
pub use self::codec::JsonCodec;
#[cfg(feature = "codec-msgpack")]
pub use self::codec::MsgpackCodec;
pub use self::{codec::Codec, error::Error};

/// An entry in the [`IsrCache`].
pub struct Entry<C>
where
    C: Codec,
{
    /// The path to the profile.
    profile_path: PathBuf,

    /// The raw profile data.
    data: Mmap,

    /// The codec used to encode and decode the profile.
    _codec: std::marker::PhantomData<C>,
}

impl<C> Entry<C>
where
    C: Codec,
{
    /// Creates a new entry from the profile path.
    pub fn new(profile_path: PathBuf) -> Result<Self, Error> {
        let data = unsafe { Mmap::map(&File::open(&profile_path)?)? };
        Ok(Self {
            profile_path,
            data,
            _codec: std::marker::PhantomData,
        })
    }

    /// Returns the path to the profile.
    pub fn profile_path(&self) -> &Path {
        &self.profile_path
    }

    /// Returns the raw profile data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Decodes the profile from the entry.
    pub fn profile(&self) -> Result<Profile<'_>, C::DecodeError> {
        C::decode_profile(&self.data)
    }

    /// Decodes the profile from the entry.
    pub fn profile_symbols(&self) -> Result<Profile<'_>, C::DecodeError> {
        C::decode_symbols(&self.data)
    }

    /// Decodes the profile from the entry.
    pub fn profile_types(&self) -> Result<Profile<'_>, C::DecodeError> {
        C::decode_types(&self.data)
    }
}

/// A cache for OS kernel profiles.
///
/// Manages the download and extraction of necessary debug symbols.
/// Uses a [`Codec`] to encode and decode profiles. The default codec is
/// [`JsonCodec`].
pub struct IsrCache<C = JsonCodec>
where
    C: Codec,
{
    #[cfg(feature = "linux")]
    ubuntu_downloader: OnceCell<UbuntuSymbolDownloader>,

    #[cfg(feature = "pdb")]
    symbol_downloader: OnceCell<SymbolDownloader>,

    /// The directory where cached profiles are stored.
    output_directory: PathBuf,

    /// The codec used to encode and decode profiles.
    _marker: std::marker::PhantomData<C>,
}

impl<C> IsrCache<C>
where
    C: Codec,
{
    /// Creates a new `IsrCache` instance, initializing it with the provided
    /// directory. If the directory doesn't exist, it attempts to create it.
    pub fn new(output_directory: impl Into<PathBuf>) -> Result<Self, Error> {
        let output_directory = output_directory.into();
        std::fs::create_dir_all(&output_directory)?;

        Ok(Self {
            #[cfg(feature = "linux")]
            ubuntu_downloader: OnceCell::new(),
            #[cfg(feature = "pdb")]
            symbol_downloader: OnceCell::new(),

            output_directory,
            _marker: std::marker::PhantomData,
        })
    }

    #[cfg(feature = "linux")]
    pub fn with_ubuntu_downloader(self, ubuntu_downloader: UbuntuSymbolDownloader) -> Self {
        Self {
            ubuntu_downloader: OnceCell::from(ubuntu_downloader),
            ..self
        }
    }

    #[cfg(feature = "linux")]
    pub fn ubuntu_downloader(&self) -> &UbuntuSymbolDownloader {
        self.ubuntu_downloader.get_or_init(|| {
            UbuntuSymbolDownloader::builder()
                .output_directory(self.output_directory.join("ubuntu"))
                .build()
        })
    }

    #[cfg(feature = "pdb")]
    pub fn with_symbol_downloader(self, symbol_downloader: SymbolDownloader) -> Self {
        Self {
            symbol_downloader: OnceCell::from(symbol_downloader),
            ..self
        }
    }

    #[cfg(feature = "pdb")]
    pub fn symbol_downloader(&self) -> &SymbolDownloader {
        self.symbol_downloader.get_or_init(|| {
            SymbolDownloader::builder()
                .output_directory(self.output_directory.join("windows"))
                .build()
        })
    }

    /// Creates or retrieves a cached profile based on a Linux kernel banner.
    ///
    /// Parses the banner to determine the kernel version and downloads the
    /// necessary debug symbols and system map if not present in the cache.
    /// Generates and stores the profile, returning its path.
    #[cfg(feature = "linux")]
    pub fn entry_from_linux_banner(&self, linux_banner: &str) -> Result<Entry<C>, Error> {
        let banner = linux_banner.parse::<LinuxBanner>()?;

        let output_paths = match banner.version_signature {
            Some(LinuxVersionSignature::Ubuntu(version_signature)) => {
                self.download_from_ubuntu_version_signature(version_signature)?
            }
            _ => return Err(Error::InvalidBanner),
        };

        let output_directory = output_paths.output_directory;

        let profile_path = output_directory
            .join("profile")
            .with_extension(C::EXTENSION);

        match File::create_new(&profile_path) {
            Ok(profile_file) => {
                let kernel_file = File::open(output_directory.join("vmlinux-dbgsym"))?;
                let systemmap_file = File::open(output_directory.join("System.map"))?;
                isr_dwarf::create_profile(kernel_file, systemmap_file, |profile| {
                    C::encode(profile_file, profile)
                })?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                tracing::debug!(
                    profile_path = %profile_path.display(),
                    "profile already exists"
                );
            }
            Err(err) => return Err(err.into()),
        }

        Entry::new(profile_path)
    }

    /// Downloads and extracts the required debug symbols from the Ubuntu
    /// repositories based on the Ubuntu version signature in the Linux banner.
    ///
    /// Returns the path to the directory containing the downloaded and
    /// extracted files.
    #[cfg(feature = "linux")]
    pub fn download_from_ubuntu_version_signature(
        &self,
        version_signature: UbuntuVersionSignature,
    ) -> Result<UbuntuSymbolPaths, isr_dl_linux::Error> {
        let request = UbuntuSymbolRequest::builder()
            .version_signature(version_signature)
            // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/linux-image.deb
            .download_linux_image_as("linux-image.deb")
            // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/vmlinuz
            .extract_linux_image_as("vmlinuz")
            // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/linux-image-dbgsym.deb
            .download_linux_image_dbgsym_as("linux-image-dbgsym.deb")
            // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/vmlinux-dbgsym
            .extract_linux_image_dbgsym_as("vmlinux-dbgsym")
            // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/linux-modules.deb
            .download_linux_modules_as("linux-modules.deb")
            // <cache>/ubuntu/6.8.0-40.40~22.04.3-generic/System.map
            .extract_systemmap_as("System.map")
            .skip_existing(true)
            .build();

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
    #[cfg(feature = "pdb")]
    pub fn entry_from_codeview(&self, codeview: CodeView) -> Result<Entry<C>, Error> {
        // <cache>/windows/ntkrnlmp.pdb/3844dbb920174967be7aa4a2c20430fa2
        let output_directory = self
            .output_directory
            .join("windows")
            .join(codeview.subdirectory());

        // <cache>/windows/ntkrnlmp.pdb/3844dbb920174967be7aa4a2c20430fa2/ntkrnlmp.pdb
        let pdb_path = self.download_from_codeview(codeview)?;

        // <cache>/windows/ntkrnlmp.pdb/3844dbb920174967be7aa4a2c20430fa2/profile<.ext>
        let profile_path = output_directory
            .join("profile")
            .with_extension(C::EXTENSION);

        match File::create_new(&profile_path) {
            Ok(profile_file) => {
                let pdb_file = File::open(&pdb_path)?;
                isr_pdb::create_profile(pdb_file, |profile| C::encode(profile_file, profile))?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                tracing::debug!(
                    profile_path = %profile_path.display(),
                    "profile already exists"
                );
            }
            Err(err) => return Err(err.into()),
        }

        Entry::new(profile_path)
    }

    /// Creates or retrieves a cached profile from a PE file.
    ///
    /// Extracts the [`CodeView`] debug information from the PE file and
    /// delegates to [`entry_from_codeview`].
    ///
    /// [`entry_from_codeview`]: Self::entry_from_codeview
    #[cfg(feature = "pdb")]
    pub fn entry_from_pe(&self, path: impl AsRef<Path>) -> Result<Entry<C>, Error> {
        self.entry_from_codeview(CodeView::from_path(path).map_err(isr_dl_pdb::Error::from)?)
    }

    #[cfg(feature = "pdb")]
    pub fn download_from_codeview(&self, codeview: CodeView) -> Result<PathBuf, Error> {
        // <cache>/windows/ntkrnlmp.pdb/3844dbb920174967be7aa4a2c20430fa2/ntkrnlmp.pdb
        let pdb_path = self
            .symbol_downloader()
            .download(SymbolRequest::builder(codeview).skip_existing(true).build())?;

        Ok(pdb_path)
    }

    /// Downloads or retrieves a cached PE binary from its [`PeInfo`].
    ///
    /// PE binaries are cached at:
    /// `<cache>/windows/<name>/<timestamp><size_of_image>/<name>`
    ///
    /// Returns the path to the cached binary.
    #[cfg(feature = "pdb")]
    pub fn download_from_image_signature(
        &self,
        image_signature: ImageSignature,
    ) -> Result<PathBuf, Error> {
        // <cache>/windows/ntoskrnl.exe/7D02613E1047000/ntoskrnl.exe
        let image_path = self.symbol_downloader().download(
            SymbolRequest::builder(image_signature)
                .skip_existing(true)
                .build(),
        )?;

        Ok(image_path)
    }
}
