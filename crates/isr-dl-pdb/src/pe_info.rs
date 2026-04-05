use std::path::Path;

use object::{
    FileKind,
    endian::LittleEndian as LE,
    read::pe::{ImageNtHeaders, ImageOptionalHeader, PeFile, PeFile32, PeFile64},
};

/// PE binary identification for symbol server downloads.
///
/// Contains the metadata needed to construct a symbol server URL
/// for downloading the original PE binary (DLL/EXE).
///
/// The symbol server URL format for PE binaries is:
/// `{server}/{name}/{timestamp}{size_of_image}/{name}`
#[derive(Debug, Clone)]
pub struct PeInfo {
    /// PE file name (e.g., "win32u.dll").
    pub name: String,

    /// TimeDateStamp from IMAGE_FILE_HEADER, as 8 uppercase hex digits.
    pub timestamp: String,

    /// SizeOfImage from IMAGE_OPTIONAL_HEADER, as lowercase hex.
    pub size_of_image: String,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Object(#[from] object::Error),

    #[error("Unsupported architecture {0:?}")]
    UnsupportedArchitecture(object::FileKind),
}

impl PeInfo {
    /// Creates a `PeInfo` from a parsed PE file and a module name.
    ///
    /// The name is provided separately because in VMI scenarios it comes
    /// from the loader data (LDR_DATA_TABLE_ENTRY.BaseDllName), not from
    /// the PE itself.
    pub fn from_pe<Pe>(pe: &PeFile<Pe>, name: impl Into<String>) -> PeInfo
    where
        Pe: ImageNtHeaders,
    {
        let timestamp = pe.nt_headers().file_header().time_date_stamp.get(LE);
        let size_of_image = pe.nt_headers().optional_header().size_of_image();

        PeInfo {
            name: name.into(),
            timestamp: format!("{timestamp:08X}"),
            size_of_image: format!("{size_of_image:x}"),
        }
    }

    /// Extracts `PeInfo` from a PE file on disk.
    pub fn from_path(path: impl AsRef<Path>) -> Result<PeInfo, Error> {
        let path = path.as_ref();
        let data = std::fs::read(path)?;

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        match FileKind::parse(&data[..])? {
            FileKind::Pe32 => Ok(Self::from_pe(&PeFile32::parse(&data[..])?, name)),
            FileKind::Pe64 => Ok(Self::from_pe(&PeFile64::parse(&data[..])?, name)),
            kind => Err(Error::UnsupportedArchitecture(kind)),
        }
    }

    /// Returns the symbol server index string (`{timestamp}{size_of_image}`).
    pub fn index(&self) -> String {
        format!("{}{}", self.timestamp, self.size_of_image)
    }
}
