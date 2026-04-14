use std::path::PathBuf;

use crate::{CodeView, ImageSignature};

/// A single artifact to fetch from a Microsoft symbol server.
pub enum SymbolRequest {
    /// Request a PDB identified by its CodeView info.
    Pdb(CodeView),

    /// Request a PE binary identified by its image signature.
    Image(ImageSignature),
}

impl SymbolRequest {
    /// Returns the basename for the symbol server lookup.
    pub fn name(&self) -> &str {
        match self {
            SymbolRequest::Pdb(request) => request.filename(),
            SymbolRequest::Image(request) => &request.name,
        }
    }

    /// Returns the second path segment for the symbol server.
    pub fn hash(&self) -> String {
        match self {
            SymbolRequest::Pdb(request) => request.hash(),
            SymbolRequest::Image(request) => request.hash(),
        }
    }

    /// Returns the relative output directory for this artifact.
    pub fn subdirectory(&self) -> PathBuf {
        match self {
            SymbolRequest::Pdb(request) => request.subdirectory(),
            SymbolRequest::Image(request) => request.subdirectory(),
        }
    }
}

impl From<CodeView> for SymbolRequest {
    fn from(value: CodeView) -> Self {
        Self::Pdb(value)
    }
}

impl From<ImageSignature> for SymbolRequest {
    fn from(value: ImageSignature) -> Self {
        Self::Image(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pdb() -> CodeView {
        CodeView {
            name: r"D:\build\kernel32.pdb".into(),
            guid: "1b72224d37b8179228200ed8994498b2".into(),
            age: 1,
        }
    }

    fn sample_image() -> ImageSignature {
        ImageSignature {
            name: "kernel32.dll".into(),
            timestamp: 0x590285E9,
            size_of_image: 0xE0000,
        }
    }

    #[test]
    fn pdb_request_strips_build_path_from_name() {
        let req: SymbolRequest = sample_pdb().into();
        assert_eq!(req.name(), "kernel32.pdb");
    }

    #[test]
    fn pdb_request_delegates_hash_to_codeview() {
        let req: SymbolRequest = sample_pdb().into();
        assert_eq!(req.hash(), sample_pdb().hash());
    }

    #[test]
    fn pdb_request_subdirectory_uses_stripped_filename() {
        let req: SymbolRequest = sample_pdb().into();
        assert_eq!(
            req.subdirectory(),
            PathBuf::from("kernel32.pdb").join("1b72224d37b8179228200ed8994498b21")
        );
    }

    #[test]
    fn image_request_exposes_full_name() {
        let req: SymbolRequest = sample_image().into();
        assert_eq!(req.name(), "kernel32.dll");
    }

    #[test]
    fn image_request_delegates_hash_to_image_signature() {
        let req: SymbolRequest = sample_image().into();
        assert_eq!(req.hash(), "590285E9e0000");
    }
}
