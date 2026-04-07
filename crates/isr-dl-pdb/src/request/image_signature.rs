use std::path::{Path, PathBuf};

use object::{
    FileKind, LittleEndian as LE,
    read::pe::{ImageNtHeaders, ImageOptionalHeader as _, PeFile, PeFile32, PeFile64},
};

use super::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageSignature {
    pub name: String,
    pub timestamp: u32,
    pub size_of_image: u32,
}

impl ImageSignature {
    pub fn from_pe<Pe>(pe: &PeFile<Pe>, name: impl Into<String>) -> Result<Self, Error>
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

    pub fn from_path(path: impl AsRef<Path>, name: impl Into<String>) -> Result<Self, Error> {
        let data = std::fs::read(path)?;

        match FileKind::parse(&data[..])? {
            FileKind::Pe32 => Self::from_pe(&PeFile32::parse(&data[..])?, name),
            FileKind::Pe64 => Self::from_pe(&PeFile64::parse(&data[..])?, name),
            kind => Err(Error::UnsupportedArchitecture(kind)),
        }
    }

    pub fn hash(&self) -> String {
        format!("{:08X}{:x}", self.timestamp, self.size_of_image)
    }

    pub fn subdirectory(&self) -> PathBuf {
        PathBuf::from(&self.name).join(self.hash())
    }
}
