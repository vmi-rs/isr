use std::path::{Path, PathBuf};

use object::{
    FileKind, LittleEndian as LE,
    read::pe::{ImageNtHeaders, ImageOptionalHeader as _, PeFile, PeFile32, PeFile64},
};

use super::DownloaderError;

/// Identifies a PE image on a Microsoft symbol server.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageSignature {
    /// Filename of the PE image, as recorded by the linker.
    pub name: String,

    /// Corresponds to `IMAGE_FILE_HEADER.TimeDateStamp`.
    pub timestamp: u32,

    /// Corresponds to `IMAGE_OPTIONAL_HEADER.SizeOfImage`.
    pub size_of_image: u32,
}

impl ImageSignature {
    /// Extracts the signature from a parsed PE file, attaching `name` as the
    /// basename for the symbol server lookup.
    pub fn from_pe<Pe>(pe: &PeFile<Pe>, name: impl Into<String>) -> Result<Self, DownloaderError>
    where
        Pe: ImageNtHeaders,
    {
        let nt_headers = pe.nt_headers();
        let file_header = nt_headers.file_header();
        let optional_header = nt_headers.optional_header();

        Ok(Self {
            name: name.into(),
            timestamp: file_header.time_date_stamp.get(LE),
            size_of_image: optional_header.size_of_image(),
        })
    }

    /// Reads the PE file at `path` and extracts its signature.
    pub fn from_path(
        path: impl AsRef<Path>,
        name: impl Into<String>,
    ) -> Result<Self, DownloaderError> {
        let data = std::fs::read(path)?;

        match FileKind::parse(&data[..])? {
            FileKind::Pe32 => Self::from_pe(&PeFile32::parse(&data[..])?, name),
            FileKind::Pe64 => Self::from_pe(&PeFile64::parse(&data[..])?, name),
            kind => Err(DownloaderError::UnsupportedFileKind(kind)),
        }
    }

    /// Returns the symbol-server lookup key.
    pub fn hash(&self) -> String {
        format!("{:08X}{:x}", self.timestamp, self.size_of_image)
    }

    /// Returns the relative output directory for this binary.
    pub fn subdirectory(&self) -> PathBuf {
        PathBuf::from(&self.name).join(self.hash())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ImageSignature {
        ImageSignature {
            name: "kernel32.dll".into(),
            timestamp: 0x590285E9,
            size_of_image: 0xE0000,
        }
    }

    #[test]
    fn hash_formats_timestamp_uppercase_and_size_lowercase() {
        assert_eq!(sample().hash(), "590285E9e0000");
    }

    #[test]
    fn hash_pads_small_timestamp_to_eight_hex_digits() {
        let sig = ImageSignature {
            timestamp: 0x1,
            size_of_image: 0x1000,
            ..sample()
        };
        assert_eq!(sig.hash(), "000000011000");
    }

    #[test]
    fn hash_uses_variable_width_size() {
        let sig = ImageSignature {
            timestamp: 0x590285E9,
            size_of_image: 0xABCDEF,
            ..sample()
        };
        assert_eq!(sig.hash(), "590285E9abcdef");
    }

    #[test]
    fn subdirectory_joins_name_and_hash() {
        assert_eq!(
            sample().subdirectory(),
            PathBuf::from("kernel32.dll").join("590285E9e0000")
        );
    }
}
