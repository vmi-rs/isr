use std::borrow::Cow;

use serde::{Deserialize, Serialize, de::IgnoredAny};

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

#[derive(Debug, Deserialize)]
pub struct ProfileSymbols<'a> {
    /// Target architecture.
    #[serde(borrow)]
    architecture: Cow<'a, str>,

    /// Symbols.
    #[serde(borrow)]
    symbols: Symbols<'a>,

    /// Types.
    #[expect(unused)]
    types: IgnoredAny,
}

impl<'a> From<ProfileSymbols<'a>> for Profile<'a> {
    fn from(profile: ProfileSymbols<'a>) -> Self {
        Self {
            architecture: profile.architecture,
            symbols: profile.symbols,
            types: Types::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ProfileTypes<'a> {
    /// Target architecture.
    #[serde(borrow)]
    architecture: Cow<'a, str>,

    /// Symbols.
    #[expect(unused)]
    symbols: IgnoredAny,

    /// Types.
    #[serde(borrow)]
    types: Types<'a>,
}

impl<'a> From<ProfileTypes<'a>> for Profile<'a> {
    fn from(profile: ProfileTypes<'a>) -> Self {
        Self {
            architecture: profile.architecture,
            symbols: Symbols::default(),
            types: profile.types,
        }
    }
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

    /// Consumes the profile and returns the symbols as owned pairs.
    ///
    /// The returned `Cow` strings preserve their original lifetime,
    /// allowing callers to extract borrowed references to the
    /// underlying data without cloning.
    pub fn into_symbols(self) -> impl IntoIterator<Item = (Cow<'a, str>, u64)> {
        self.symbols.0
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
            Type::Array(r) => self
                .type_size(&r.subtype)
                .map(|subtype_size| subtype_size * r.dims.iter().product::<u64>()),
            Type::Pointer(r) => Some(r.size),
            Type::Bitfield(r) => self.type_size(&r.subtype),
            Type::Function => Some(self.pointer_size()),
        }
    }

    /// Returns the size of a base type in bytes.
    pub fn base_size(&self, base: &BaseRef) -> u64 {
        base.size()
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
