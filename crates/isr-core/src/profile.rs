use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::{
    symbols::Symbols,
    types::{BaseRef, Enum, Struct, Type, Types},
};

/// Profile.
///
/// Contains information about the target architecture, symbols, and types.
#[derive(Debug, Serialize, Deserialize)]
pub struct Profile<'a> {
    /// Target architecture.
    #[serde(borrow)]
    architecture: Cow<'a, str>,

    /// Symbols.
    #[serde(borrow)]
    symbols: Symbols<'a>,

    /// Types.
    #[serde(borrow)]
    types: Types<'a>,
}

impl<'a> Profile<'a> {
    /// Creates a new profile.
    pub fn new(architecture: Cow<'a, str>, symbols: Symbols<'a>, types: Types<'a>) -> Self {
        Self {
            architecture,
            symbols,
            types,
        }
    }

    /// Returns an iterator over the symbols.
    pub fn symbols(&self) -> impl Iterator<Item = (&str, &u64)> {
        self.symbols
            .0
            .iter()
            .map(|(name, value)| (name.as_ref(), value))
    }

    /// Returns the types.
    pub fn types(&self) -> &Types<'_> {
        &self.types
    }

    /// Returns the size of a given type in bytes.
    pub fn type_size(&self, type_: &Type) -> Option<u64> {
        match type_ {
            Type::Base(r) => Some(self.base_size(r)),
            Type::Enum(r) => self.enum_size(&r.name),
            Type::Struct(r) => self.struct_size(&r.name),
            Type::Array(r) => self.type_size(&r.subtype),
            Type::Pointer(_) => Some(self.pointer_size()),
            Type::Bitfield(r) => self.type_size(&r.subtype),
            Type::Function => Some(self.pointer_size()),
        }
    }

    /// Returns the size of a base type in bytes.
    pub fn base_size(&self, base: &BaseRef) -> u64 {
        match base {
            BaseRef::Void => 0,
            BaseRef::Bool | BaseRef::Char | BaseRef::I8 | BaseRef::U8 | BaseRef::F8 => 1,
            BaseRef::Wchar | BaseRef::I16 | BaseRef::U16 | BaseRef::F16 => 2,
            BaseRef::I32 | BaseRef::U32 | BaseRef::F32 => 4,
            BaseRef::I64 | BaseRef::U64 | BaseRef::F64 => 8,
            BaseRef::I128 | BaseRef::U128 | BaseRef::F128 => 16,
        }
    }

    /// Returns the size of an enum type in bytes.
    pub fn enum_size(&self, name: &str) -> Option<u64> {
        self.type_size(&self.types.enums.get(name)?.subtype)
    }

    /// Returns the size of a struct type in bytes.
    pub fn struct_size(&self, name: &str) -> Option<u64> {
        self.types.structs.get(name).map(|udt| udt.size)
    }

    /// Returns the size of a pointer in bytes.
    pub fn pointer_size(&self) -> u64 {
        match self.architecture.as_ref() {
            "X86" | "Arm" => 4,
            "Amd64" | "Arm64" => 8,
            _ => panic!("unsupported architecture"),
        }
    }

    /// Finds a symbol by name.
    pub fn find_symbol(&self, symbol_name: &str) -> Option<u64> {
        self.symbols.0.get(symbol_name).copied()
    }

    /// Finds an enum by name.
    pub fn find_enum(&self, type_name: &str) -> Option<&Enum<'_>> {
        self.types.enums.get(type_name)
    }

    /// Finds a struct by name.
    pub fn find_struct(&self, type_name: &str) -> Option<&Struct<'_>> {
        self.types.structs.get(type_name)
    }
}
