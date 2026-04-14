//! Owned, serializable data model for a profile.
//!
//! These types form the on-disk schema. They are archived via rkyv and
//! consumed through the typed view in [`crate::Profile`].

use indexmap::IndexMap;
use rkyv::{Archive, Deserialize, Serialize};

/// A full profile: architecture, type definitions, and symbols.
#[derive(Debug, Default, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[rkyv(derive(Debug))]
pub struct Profile {
    /// Target architecture.
    pub architecture: Architecture,

    /// Enum definitions keyed by name.
    pub enums: IndexMap<String, Enum>,

    /// Struct definitions keyed by name.
    pub structs: IndexMap<String, Struct>,

    /// Symbol RVAs keyed by symbol name.
    pub symbols: IndexMap<String, u64>,
}

/// Target CPU architecture.
#[derive(Debug, Default, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[rkyv(derive(Debug))]
pub enum Architecture {
    /// Unknown or unspecified architecture.
    #[default]
    Unknown,

    /// 32-bit x86.
    X86,

    /// 64-bit x86.
    Amd64,

    /// 32-bit ARM.
    Arm32,

    /// 64-bit ARM.
    Arm64,
}

//
// Enum
//

/// Enum type.
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[rkyv(derive(Debug))]
pub struct Enum {
    /// Underlying integer type of the enum.
    pub subtype: Type,

    /// Enum variants keyed by name.
    pub fields: IndexMap<String, Variant>,
}

/// Enum variant.
#[allow(missing_docs)]
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(untagged)
)]
#[rkyv(derive(Debug))]
pub enum Variant {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
}

//
// Struct
//

/// Struct type.
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[rkyv(derive(Debug))]
pub struct Struct {
    /// Struct / class / union / interface tag.
    pub kind: StructKind,

    /// Size of the struct in bytes.
    pub size: u64,

    /// Fields keyed by name.
    pub fields: IndexMap<String, Field>,
}

/// Struct kind.
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[rkyv(derive(Debug))]
pub enum StructKind {
    /// A `struct`.
    Struct,

    /// A `class`.
    Class,

    /// A `union`.
    Union,

    /// An `interface`.
    Interface,
}

/// Struct field.
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[rkyv(derive(Debug))]
pub struct Field {
    /// Field offset (in bytes).
    pub offset: u64,

    /// Field type.
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub ty: Type,
}

//
// Type
//

/// Type.
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case", tag = "kind")
)]
#[rkyv(derive(Debug))]
pub enum Type {
    /// Base type.
    Base(Base),

    /// Enum type.
    Enum(EnumRef),

    /// Struct type.
    Struct(StructRef),

    /// Array type.
    Array(Array),

    /// Pointer type.
    Pointer(Pointer),

    /// Bitfield type.
    Bitfield(Bitfield),

    /// Function type.
    Function,
}

/// Base type.
#[allow(missing_docs)]
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case", tag = "subkind")
)]
#[rkyv(derive(Debug))]
pub enum Base {
    /// Void type.
    Void,

    /// Boolean type.
    Bool,

    /// Character types.
    Char8,
    Char16,
    Char32,

    /// Signed integer types.
    I8,
    I16,
    I32,
    I64,
    I128,

    /// Unsigned integer types.
    U8,
    U16,
    U32,
    U64,
    U128,

    /// Floating-point types.
    F8,
    F16,
    F32,
    F64,
    F128,
}

impl Base {
    /// Returns the size of the base type in bytes.
    pub fn size(&self) -> u64 {
        match self {
            Self::Void => 0,
            Self::Char8 | Self::I8 | Self::U8 | Self::F8 | Self::Bool => 1,
            Self::Char16 | Self::I16 | Self::U16 | Self::F16 => 2,
            Self::Char32 | Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
            Self::I128 | Self::U128 | Self::F128 => 16,
        }
    }
}

impl ArchivedBase {
    /// Returns the size of the base type in bytes.
    pub fn size(&self) -> u64 {
        match self {
            Self::Void => 0,
            Self::Char8 | Self::I8 | Self::U8 | Self::F8 | Self::Bool => 1,
            Self::Char16 | Self::I16 | Self::U16 | Self::F16 => 2,
            Self::Char32 | Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
            Self::I128 | Self::U128 | Self::F128 => 16,
        }
    }
}

/// Enum reference.
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[rkyv(derive(Debug))]
pub struct EnumRef {
    /// Name of the referenced enum.
    pub name: String,
}

/// Struct reference.
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[rkyv(derive(Debug))]
pub struct StructRef {
    /// Name of the referenced struct.
    pub name: String,
}

/// Array type.
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[rkyv(
    derive(Debug),
    serialize_bounds(
        __S: rkyv::ser::Writer + rkyv::ser::Allocator,
        __S::Error: rkyv::rancor::Source,
    ),
    deserialize_bounds(
        __D::Error: rkyv::rancor::Source,
    ),
    bytecheck(bounds(
        __C: rkyv::validation::ArchiveContext,
        __C::Error: rkyv::rancor::Source,
    )
))]
pub struct Array {
    /// Element type.
    #[rkyv(omit_bounds)]
    pub subtype: Box<Type>,

    /// Array dimensions.
    pub dims: Vec<u64>,
}

/// Bitfield type.
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[rkyv(
    derive(Debug),
    serialize_bounds(
        __S: rkyv::ser::Writer + rkyv::ser::Allocator,
        __S::Error: rkyv::rancor::Source,
    ),
    deserialize_bounds(
        __D::Error: rkyv::rancor::Source,
    ),
    bytecheck(bounds(
        __C: rkyv::validation::ArchiveContext,
        __C::Error: rkyv::rancor::Source,
    )
))]
pub struct Bitfield {
    /// Bitfield subtype.
    #[rkyv(omit_bounds)]
    pub subtype: Box<Type>,

    /// Bit length.
    pub bit_length: u64,

    /// Bit position.
    pub bit_position: u64,
}

/// Pointer type.
#[derive(Debug, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[rkyv(
    derive(Debug),
    serialize_bounds(
        __S: rkyv::ser::Writer + rkyv::ser::Allocator,
        __S::Error: rkyv::rancor::Source,
    ),
    deserialize_bounds(
        __D::Error: rkyv::rancor::Source,
    ),
    bytecheck(bounds(
        __C: rkyv::validation::ArchiveContext,
        __C::Error: rkyv::rancor::Source,
    )
))]
pub struct Pointer {
    /// Type of the pointed value.
    #[rkyv(omit_bounds)]
    pub subtype: Box<Type>,

    /// Size of the pointer in bytes.
    pub size: u64,
}
