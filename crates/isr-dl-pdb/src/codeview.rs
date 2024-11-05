use std::path::Path;

use object::{
    read::pe::{ImageNtHeaders, PeFile, PeFile32, PeFile64},
    FileKind, Object,
};

/// CodeView information extracted from a PDB file.
#[derive(Debug, Clone)]
pub struct CodeView {
    /// Path to the PDB file.
    pub path: String,

    /// PDB GUID.
    pub guid: String,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Object(#[from] object::Error),

    #[error("Unsupported architecture {0:?}")]
    UnsupportedArchitecture(object::FileKind),

    #[error("CodeView not found")]
    NotFound,
}

impl CodeView {
    pub fn from_pe<Pe>(pe: &PeFile<Pe>) -> Result<CodeView, Error>
    where
        Pe: ImageNtHeaders,
    {
        let cv = match pe.pdb_info()? {
            Some(cv) => cv,
            None => return Err(Error::NotFound),
        };

        let guid = cv.guid();
        let age = cv.age();
        let path = cv.path();

        let guid0 = u32::from_le_bytes(guid[0..4].try_into().unwrap());
        let guid1 = u16::from_le_bytes(guid[4..6].try_into().unwrap());
        let guid2 = u16::from_le_bytes(guid[6..8].try_into().unwrap());
        let guid3 = &guid[8..16];

        Ok(CodeView {
            path: String::from_utf8_lossy(path).to_string(),
            guid: format!(
                "{:08x}{:04x}{:04x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:01x}",
                guid0,
                guid1,
                guid2,
                guid3[0],
                guid3[1],
                guid3[2],
                guid3[3],
                guid3[4],
                guid3[5],
                guid3[6],
                guid3[7],
                age & 0xf,
            ),
        })
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<CodeView, Error> {
        let data = std::fs::read(path)?;

        match FileKind::parse(&data[..])? {
            FileKind::Pe32 => Self::from_pe(&PeFile32::parse(&data[..])?),
            FileKind::Pe64 => Self::from_pe(&PeFile64::parse(&data[..])?),
            kind => Err(Error::UnsupportedArchitecture(kind)),
        }
    }
}
