mod codeview;
mod error;
mod image_signature;

use std::path::PathBuf;

pub use self::{codeview::CodeView, error::Error, image_signature::ImageSignature};

pub enum SymbolKind {
    Pdb(CodeView),
    Pe(ImageSignature),
}

impl SymbolKind {
    pub fn name(&self) -> &str {
        match self {
            SymbolKind::Pdb(request) => &request.name,
            SymbolKind::Pe(request) => &request.name,
        }
    }

    pub fn hash(&self) -> String {
        match self {
            SymbolKind::Pdb(request) => request.hash(),
            SymbolKind::Pe(request) => request.hash(),
        }
    }

    pub fn subdirectory(&self) -> PathBuf {
        match self {
            SymbolKind::Pdb(request) => request.subdirectory(),
            SymbolKind::Pe(request) => request.subdirectory(),
        }
    }
}

impl From<CodeView> for SymbolKind {
    fn from(value: CodeView) -> Self {
        Self::Pdb(value)
    }
}

impl From<ImageSignature> for SymbolKind {
    fn from(value: ImageSignature) -> Self {
        Self::Pe(value)
    }
}
