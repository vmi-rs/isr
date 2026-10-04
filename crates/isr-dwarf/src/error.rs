/// Errors surfaced while building a profile from DWARF debug info.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// I/O error while reading the kernel image, dSYM or `System.map`.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Failed to parse the kernel image or dSYM as an object file.
    #[error(transparent)]
    Object(#[from] object::Error),

    /// Error from the `gimli` DWARF parser.
    #[error(transparent)]
    Gimli(#[from] gimli::Error),

    /// The supplied `System.map` is not in the expected format.
    #[error("invalid system map")]
    InvalidSystemMap,

    /// The supplied dSYM is not a Mach-O file.
    #[error("unsupported object file format {0:?} (expected Mach-O)")]
    UnsupportedFormat(object::BinaryFormat),

    /// The supplied universal binary has no arm64 slice.
    #[error("universal binary has no arm64 slice")]
    MissingArm64Slice,

    /// Failed to serialize the generated profile.
    #[error("serialization error: {0}")]
    Serialization(Box<dyn std::error::Error + Send + Sync>),
}
