use std::path::{Path, PathBuf};

use object::{
    FileKind, Object,
    read::pe::{ImageNtHeaders, PeFile, PeFile32, PeFile64},
};

use super::DownloaderError;

/// Identifies the PDB matching a specific PE binary.
///
/// Parsed from the CodeView entry in the PE's debug directory.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CodeView {
    /// The PDB path as recorded by the linker.
    ///
    /// Can be a full build path or just a filename.
    pub name: String,

    /// PDB signature GUID.
    pub guid: String,

    /// PDB age counter.
    pub age: u32,
}

impl CodeView {
    /// Extracts CodeView info from a parsed PE file. Returns
    /// [`DownloaderError::MissingCodeView`] if the PE has no debug directory.
    pub fn from_pe<Pe>(pe: &PeFile<Pe>) -> Result<Self, DownloaderError>
    where
        Pe: ImageNtHeaders,
    {
        let cv = match pe.pdb_info()? {
            Some(cv) => cv,
            None => return Err(DownloaderError::MissingCodeView),
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

    /// Reads the PE file at `path` and extracts its CodeView info.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, DownloaderError> {
        let data = std::fs::read(path)?;

        match FileKind::parse(&data[..])? {
            FileKind::Pe32 => Self::from_pe(&PeFile32::parse(&data[..])?),
            FileKind::Pe64 => Self::from_pe(&PeFile64::parse(&data[..])?),
            kind => Err(DownloaderError::UnsupportedFileKind(kind)),
        }
    }

    /// Returns the PDB filename without any directory components.
    pub fn filename(&self) -> &str {
        match self.name.rsplit_once(['\\', '/']) {
            Some((_, filename)) => filename,
            None => &self.name,
        }
    }

    /// Returns the symbol-server lookup key `<guid><age>` used as the path
    /// segment between the PDB name and the downloaded file.
    pub fn hash(&self) -> String {
        format!("{}{:x}", self.guid, self.age)
    }

    /// Returns the relative output directory for this PDB:
    /// `<filename>/<hash>`.
    pub fn subdirectory(&self) -> PathBuf {
        PathBuf::from(self.filename()).join(self.hash())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CodeView {
        CodeView {
            name: "kernel32.pdb".into(),
            guid: "1b72224d37b8179228200ed8994498b2".into(),
            age: 1,
        }
    }

    #[test]
    fn hash_concatenates_guid_and_age_in_hex() {
        assert_eq!(sample().hash(), "1b72224d37b8179228200ed8994498b21");
    }

    #[test]
    fn hash_formats_age_as_lowercase_hex() {
        let cv = CodeView {
            age: 0xab,
            ..sample()
        };
        assert!(cv.hash().ends_with("ab"));
    }

    #[test]
    fn subdirectory_joins_filename_and_hash() {
        assert_eq!(
            sample().subdirectory(),
            PathBuf::from("kernel32.pdb").join("1b72224d37b8179228200ed8994498b21")
        );
    }

    #[test]
    fn filename_strips_windows_style_path() {
        let cv = CodeView {
            name: r"D:\a\_work\1\s\msvcp140.amd64.pdb".into(),
            ..sample()
        };
        assert_eq!(cv.filename(), "msvcp140.amd64.pdb");
    }

    #[test]
    fn filename_strips_unix_style_path() {
        let cv = CodeView {
            name: "path/to/foo.pdb".into(),
            ..sample()
        };
        assert_eq!(cv.filename(), "foo.pdb");
    }

    #[test]
    fn filename_returns_name_when_no_separator() {
        assert_eq!(sample().filename(), "kernel32.pdb");
    }

    #[test]
    fn subdirectory_uses_filename_not_full_name() {
        let cv = CodeView {
            name: r"D:\build\msvcp140.amd64.pdb".into(),
            ..sample()
        };
        assert_eq!(
            cv.subdirectory(),
            PathBuf::from("msvcp140.amd64.pdb").join("1b72224d37b8179228200ed8994498b21")
        );
    }
}
