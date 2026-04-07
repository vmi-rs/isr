use std::path::{Path, PathBuf};

use object::{
    FileKind, Object,
    read::pe::{ImageNtHeaders, PeFile, PeFile32, PeFile64},
};

use super::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CodeView {
    pub name: String,
    pub guid: String,
    pub age: u32,
}

impl CodeView {
    pub fn from_pe<Pe>(pe: &PeFile<Pe>) -> Result<Self, Error>
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

        Ok(Self {
            name: String::from_utf8_lossy(path).to_string(),
            guid: format!(
                "{:08x}{:04x}{:04x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
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
            ),
            age: age & 0xf,
        })
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        let data = std::fs::read(path)?;

        match FileKind::parse(&data[..])? {
            FileKind::Pe32 => Self::from_pe(&PeFile32::parse(&data[..])?),
            FileKind::Pe64 => Self::from_pe(&PeFile64::parse(&data[..])?),
            kind => Err(Error::UnsupportedArchitecture(kind)),
        }
    }

    pub fn hash(&self) -> String {
        format!("{}{:x}", self.guid, self.age)
    }

    pub fn subdirectory(&self) -> PathBuf {
        PathBuf::from(&self.name).join(self.hash())
    }
}
