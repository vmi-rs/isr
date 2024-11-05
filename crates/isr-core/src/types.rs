//! Types module.
//!
//! This module contains the types used to represent the data structures of the
//! profile and symbols files.

use std::borrow::Cow;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Types<'a> {
    #[serde(borrow)]
    pub enums: IndexMap<Cow<'a, str>, Enum<'a>>,
    #[serde(borrow)]
    pub structs: IndexMap<Cow<'a, str>, Struct<'a>>,
}

//
// Enum
//

/// Enum type.
#[derive(Debug, Serialize, Deserialize)]
pub struct Enum<'a> {
    #[serde(borrow)]
    pub subtype: Type<'a>,
    #[serde(borrow)]
    pub fields: IndexMap<Cow<'a, str>, Variant>,
}

/// Enum variant.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
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
#[derive(Debug, Serialize, Deserialize)]
pub struct Struct<'a> {
    pub kind: StructKind,
    pub size: u64,
    #[serde(borrow)]
    pub fields: IndexMap<Cow<'a, str>, Field<'a>>,
}

/// Struct kind.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Serialize, Deserialize)]
pub struct Field<'a> {
    /// Field offset (in bytes).
    pub offset: u64,

    /// Field type.
    #[serde(borrow, rename = "type")]
    pub type_: Type<'a>,
}

//
// Type
//

/// Type.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Type<'a> {
    /// Base type.
    Base(BaseRef),

    /// Enum type.
    Enum(#[serde(borrow)] EnumRef<'a>),

    /// Struct type.
    Struct(#[serde(borrow)] StructRef<'a>),

    /// Array type.
    Array(#[serde(borrow)] ArrayRef<'a>),

    /// Pointer type.
    Pointer(#[serde(borrow)] PointerRef<'a>),

    /// Bitfield type.
    Bitfield(#[serde(borrow)] BitfieldRef<'a>),

    /// Function type.
    Function,
}

/// Base type reference.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "subkind")]
pub enum BaseRef {
    /// Void type.
    Void,

    /// Boolean type.
    Bool,

    /// Character type.
    Char,

    /// Wide character type.
    Wchar,

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

/// Enum reference.
#[derive(Debug, Serialize, Deserialize)]
pub struct EnumRef<'a> {
    /// Name of the referenced enum.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
}

/// Struct reference.
#[derive(Debug, Serialize, Deserialize)]
pub struct StructRef<'a> {
    /// Name of the referenced struct.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
}

/// Array reference.
#[derive(Debug, Serialize, Deserialize)]
pub struct ArrayRef<'a> {
    /// Element type.
    #[serde(borrow)]
    pub subtype: Box<Type<'a>>,

    /// Array dimensions.
    pub dims: SmallVec<[u64; 4]>,

    /// Total number of elements.
    pub size: u64,
}

/// Bitfield reference.
#[derive(Debug, Serialize, Deserialize)]
pub struct BitfieldRef<'a> {
    /// Bitfield subtype.
    #[serde(borrow)]
    pub subtype: Box<Type<'a>>,

    /// Bit length.
    pub bit_length: u64,

    /// Bit position.
    pub bit_position: u64,
}

/// Pointer reference.
#[derive(Debug, Serialize, Deserialize)]
pub struct PointerRef<'a> {
    /// Type of the pointed value.
    #[serde(borrow)]
    pub subtype: Box<Type<'a>>,
}
