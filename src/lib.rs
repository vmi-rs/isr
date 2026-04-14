//! # Intermediate Symbol Representation
//!
//! The `isr` crate provides a unified, version-agnostic way to access and
//! utilize debugging symbols from various sources, including PDB files
//! (Windows) and DWARF debug info (Linux). This allows developers to write
//! code that interacts with different operating system versions without
//! needing to hardcode offsets or constantly update for new releases.
//!
//! ## Features
//!
//! - **Unified Representation:** Abstracts away the underlying symbol format
//!   (PDB, DWARF) into a common, easy-to-use structure.
//!
//! - **Version Agnostic:** Enables writing code that works seamlessly across
//!   different OS versions, avoiding the need for version-specific logic.
//!
//! - **Fast Symbol Parsing:** The ISR parsing process is highly optimized for
//!   speed, enabling quick access to symbol information.
//!
//! - **Automated Symbol Download and Caching:** For Windows, automatically
//!   downloads and caches PDB symbols based on CodeView information extracted
//!   from executables or the kernel itself.
//!
//!   For Linux (currently Ubuntu), automatically downloads and extracts the
//!   kernel debug symbols and the `System.map` file.
//!
//! - **Convenient Macros:** Provides [`symbols!`] and [`offsets!`] macros for
//!   streamlined symbol definition and type-safe access in your code.
//!
//! - **Zero-copy On-disk Format:** Profiles are stored as rkyv-archived blobs
//!   and accessed via mmap without an explicit deserialize pass.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use isr::{
//!     download::windows::CodeView,
//!     macros::{symbols, offsets, Field},
//!     IsrCache,
//! };
//!
//! symbols! {
//!     struct Symbols {
//!         NtCreateFile: u64,
//!     }
//! }
//!
//! offsets! {
//!     struct Offsets {
//!         struct _EPROCESS {
//!             UniqueProcessId: Field,
//!         }
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a cache instance.
//! let cache = IsrCache::new("cache")?;
//!
//! // Use the CodeView information of the Windows 10.0.18362.356 kernel.
//! let entry = cache.entry_from_codeview(CodeView {
//!     name: String::from("ntkrnlmp.pdb"),
//!     guid: String::from("ce7ffb00c20b87500211456b3e905c47"),
//!     age: 1,
//! })?;
//!
//! // You can also use `entry_from_pe` method:
//! // let entry = cache.entry_from_pe("path/to/ntoskrnl.exe")?;
//!
//! let profile = entry.profile()?;
//!
//! // Instantiate your symbol and offset structures using the profile.
//! let symbols = Symbols::new(&profile)?;
//! let offsets = Offsets::new(&profile)?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Downloading Ubuntu Kernel Profiles
//!
//! Downloading the required files for creating an Ubuntu kernel profile can
//! take a considerable amount of time due to the large size of the debug symbol
//! packages. The download may exceed 1GB of data.
//!
//! ## Additional Notes
//!
//! People familiar with the Volatility 3 or Rekall framework might notice that
//! the ISR format shares some conceptual similarities with their profiles (or
//! Intermediate Symbol Format (ISF), respectively). All of them aim to provide
//! a unified representation of kernel symbols and structures. However, it's
//! crucial to note that the ISR format is **not** compatible with other
//! frameworks. ISR is specifically designed for the [`vmi`] crate.
//!
//! # License
//!
//! This project is licensed under the MIT license.
//!
//! [`symbols!`]: crate::macros::symbols
//! [`offsets!`]: crate::macros::offsets
//! [`vmi`]: ../vmi/index.html

pub use isr_core::*;

pub mod macros {
    #![doc = include_str!("../docs/isr-macros.md")]

    pub use isr_macros::*;
}

#[cfg(feature = "cache")]
pub mod cache {
    #![doc = include_str!("../docs/isr-cache.md")]

    pub use isr_cache::*;
}

// Re-export the `IsrCache` to the root of the crate.
#[cfg(feature = "cache")]
#[doc(inline)]
pub use self::cache::IsrCache;

#[cfg(feature = "pdb")]
pub mod pdb {
    #![doc = include_str!("../docs/isr-pdb.md")]

    pub use isr_pdb::*;
}

#[cfg(feature = "dwarf")]
pub mod dwarf {
    #![doc = include_str!("../docs/isr-dwarf.md")]

    pub use isr_dwarf::*;
}

#[cfg(any(feature = "dl-linux", feature = "dl-windows"))]
pub mod download {
    //! Downloaders for various symbol formats.

    pub use isr_dl::*;

    #[cfg(feature = "dl-linux")]
    pub mod linux {
        #![doc = include_str!("../docs/isr-dl-linux.md")]

        pub use isr_dl_linux::*;
    }

    #[cfg(feature = "dl-windows")]
    pub mod windows {
        #![doc = include_str!("../docs/isr-dl-windows.md")]

        pub use isr_dl_windows::*;
    }
}
