//! Struct field decoding.
//!
//! Walks a [`Struct`] over a byte slice and produces a [`FieldValue`] (type
//! plus decoded value) or [`FieldValueKind`] (type only) per field.

use crate::{Base, Profile, Struct, Type};

/// A struct field's decoded type and value.
#[allow(missing_docs)]
pub enum FieldValue<'a> {
    Void,
    Bool(bool),
    Char8(u8),
    Char16(u16),
    Char32(u32),

    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),

    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),

    F8(u8),
    F16(u16),
    F32(f32),
    F64(f64),
    F128(u128),

    Enum {
        type_name: &'a str,
        size: u64,
        value: u64,
    },
    Struct {
        type_name: &'a str,
        size: u64,
        data: &'a [u8],
    },
    Array {
        subtype: Box<FieldValueKind<'a>>,
        dims: Vec<u64>,
        size: u64,
        data: &'a [u8],
    },
    Pointer {
        subtype: Box<FieldValueKind<'a>>,
        size: u64,
        value: u64,
    },
    Bitfield {
        bit_position: u64,
        bit_length: u64,
        size: u64,
        value: u64,
    },
    Function {
        value: u64,
    },
}

impl FieldValue<'_> {
    /// Returns the size of the field value in bytes.
    pub fn size(&self) -> u64 {
        match self {
            Self::Void => 0,
            Self::Char8(_) | Self::I8(_) | Self::U8(_) | Self::F8(_) | Self::Bool(_) => 1,
            Self::Char16(_) | Self::I16(_) | Self::U16(_) | Self::F16(_) => 2,
            Self::Char32(_) | Self::I32(_) | Self::U32(_) | Self::F32(_) => 4,
            Self::I64(_) | Self::U64(_) | Self::F64(_) => 8,
            Self::I128(_) | Self::U128(_) | Self::F128(_) => 16,
            Self::Enum { size, .. } => *size,
            Self::Struct { size, .. } => *size,
            Self::Array { size, .. } => *size,
            Self::Pointer { size, .. } => *size,
            Self::Bitfield { size, .. } => *size,
            Self::Function { .. } => 0,
        }
    }
}

/// A struct field's type, without a decoded value.
#[allow(missing_docs)]
pub enum FieldValueKind<'a> {
    Void,
    Bool,
    Char8,
    Char16,
    Char32,

    I8,
    I16,
    I32,
    I64,
    I128,

    U8,
    U16,
    U32,
    U64,
    U128,

    F8,
    F16,
    F32,
    F64,
    F128,

    Enum {
        type_name: &'a str,
        size: u64,
    },
    Struct {
        type_name: &'a str,
        size: u64,
    },
    Array {
        subtype: Box<FieldValueKind<'a>>,
        dims: Vec<u64>,
        size: u64,
    },
    Pointer {
        subtype: Box<FieldValueKind<'a>>,
        size: u64,
    },
    Bitfield {
        bit_position: u64,
        bit_length: u64,
        size: u64,
    },
    Function,
}

impl FieldValueKind<'_> {
    /// Returns the size of the field value kind in bytes.
    pub fn size(&self) -> u64 {
        match self {
            Self::Void => 0,
            Self::Char8 | Self::I8 | Self::U8 | Self::F8 | Self::Bool => 1,
            Self::Char16 | Self::I16 | Self::U16 | Self::F16 => 2,
            Self::Char32 | Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
            Self::I128 | Self::U128 | Self::F128 => 16,
            Self::Enum { size, .. } => *size,
            Self::Struct { size, .. } => *size,
            Self::Array { size, .. } => *size,
            Self::Pointer { size, .. } => *size,
            Self::Bitfield { size, .. } => *size,
            Self::Function { .. } => 0,
        }
    }
}

impl<'a> From<FieldValue<'a>> for FieldValueKind<'a> {
    fn from(value: FieldValue<'a>) -> Self {
        match value {
            FieldValue::Void => Self::Void,
            FieldValue::Bool(_) => Self::Bool,
            FieldValue::Char8(_) => Self::Char8,
            FieldValue::Char16(_) => Self::Char16,
            FieldValue::Char32(_) => Self::Char32,

            FieldValue::I8(_) => Self::I8,
            FieldValue::I16(_) => Self::I16,
            FieldValue::I32(_) => Self::I32,
            FieldValue::I64(_) => Self::I64,
            FieldValue::I128(_) => Self::I128,

            FieldValue::U8(_) => Self::U8,
            FieldValue::U16(_) => Self::U16,
            FieldValue::U32(_) => Self::U32,
            FieldValue::U64(_) => Self::U64,
            FieldValue::U128(_) => Self::U128,

            FieldValue::F8(_) => Self::F8,
            FieldValue::F16(_) => Self::F16,
            FieldValue::F32(_) => Self::F32,
            FieldValue::F64(_) => Self::F64,
            FieldValue::F128(_) => Self::F128,

            FieldValue::Enum {
                type_name, size, ..
            } => Self::Enum { type_name, size },
            FieldValue::Struct {
                type_name, size, ..
            } => Self::Struct { type_name, size },
            FieldValue::Array {
                subtype,
                dims,
                size,
                ..
            } => Self::Array {
                subtype: Box::new(*subtype),
                dims,
                size,
            },
            FieldValue::Pointer { subtype, size, .. } => Self::Pointer {
                subtype: Box::new(*subtype),
                size,
            },
            FieldValue::Bitfield {
                bit_position,
                bit_length,
                size,
                ..
            } => Self::Bitfield {
                bit_position,
                bit_length,
                size,
            },
            FieldValue::Function { .. } => Self::Function,
        }
    }
}

/// A struct field with its decoded value.
pub struct StructField<'a> {
    /// Field name.
    pub name: &'a str,

    /// Field offset from the start of the struct, in bytes.
    pub offset: usize,

    /// Decoded type and value at `offset`.
    pub value: FieldValue<'a>,
}

/// A struct field described by its type only.
pub struct StructFieldKind<'a> {
    /// Field name.
    pub name: &'a str,

    /// Field offset from the start of the struct, in bytes.
    pub offset: usize,

    /// Type at `offset`.
    pub value: FieldValueKind<'a>,
}

/// Decodes each field of `struct_def` from `data` and invokes `visitor`.
pub fn visit_struct(
    profile: &Profile,
    struct_def: &Struct,
    mut visitor: impl FnMut(&StructField),
    data: &[u8],
) {
    for field in struct_def.fields() {
        let offset = field.offset() as usize;
        let value = make_value(profile, field.ty(), data, offset);

        visitor(&StructField {
            name: field.name(),
            offset,
            value,
        });
    }
}

/// Walks each field of `struct_def` by type only, without reading data.
pub fn visit_struct_only(
    profile: &Profile,
    struct_def: Struct,
    mut visitor: impl FnMut(&StructFieldKind),
) {
    for field in struct_def.fields() {
        let offset = field.offset() as usize;
        let value = make_value_kind(profile, field.ty());

        visitor(&StructFieldKind {
            name: field.name(),
            offset,
            value,
        });
    }
}

#[expect(clippy::unnecessary_cast)]
fn make_value<'a>(
    profile: &Profile,
    ty: Type<'a>,
    data: &'a [u8],
    offset: usize,
) -> FieldValue<'a> {
    match &ty {
        Type::Base(base) => match base {
            Base::Void => FieldValue::Void,
            Base::Bool => FieldValue::Bool(read_uint(data, offset, 1) != 0),
            Base::Char8 => FieldValue::Char8(read_uint(data, offset, 1) as u8),
            Base::Char16 => FieldValue::Char16(read_uint(data, offset, 2) as u16),
            Base::Char32 => FieldValue::Char32(read_uint(data, offset, 4) as u32),

            Base::I8 => FieldValue::I8(read_uint(data, offset, 1) as i8),
            Base::I16 => FieldValue::I16(read_uint(data, offset, 2) as u16 as i16),
            Base::I32 => FieldValue::I32(read_uint(data, offset, 4) as u32 as i32),
            Base::I64 => FieldValue::I64(read_uint(data, offset, 8) as i64),
            Base::I128 => FieldValue::I128(read_uint(data, offset, 16) as i128),

            Base::U8 => FieldValue::U8(read_uint(data, offset, 1) as u8),
            Base::U16 => FieldValue::U16(read_uint(data, offset, 2) as u16),
            Base::U32 => FieldValue::U32(read_uint(data, offset, 4) as u32),
            Base::U64 => FieldValue::U64(read_uint(data, offset, 8) as u64),
            Base::U128 => FieldValue::U128(read_uint(data, offset, 16) as u128),
            _ => {
                let size = base.size() as usize;
                let val = read_uint(data, offset, size);
                match size {
                    1 => FieldValue::U8(val as u8),
                    2 => FieldValue::U16(val as u16),
                    4 => FieldValue::U32(val as u32),
                    8 => FieldValue::U64(val as u64),
                    16 => FieldValue::U128(val as u128),
                    _ => FieldValue::Void,
                }
            }
        },
        Type::Struct(sref) => FieldValue::Struct {
            type_name: sref.name(),
            size: profile.type_size(ty).unwrap_or(0),
            data: &data[offset..],
        },
        Type::Pointer(ptr) => FieldValue::Pointer {
            subtype: Box::new(make_value_kind(profile, ptr.subtype())),
            size: profile.type_size(ty).unwrap_or(0),
            value: read_uint(data, offset, ptr.size() as usize) as u64,
        },
        Type::Array(arr) => FieldValue::Array {
            subtype: Box::new(make_value_kind(profile, arr.subtype())),
            dims: arr.dims().collect(),
            size: profile.type_size(ty).unwrap_or(0),
            data: &data[offset..],
        },
        Type::Bitfield(bf) => {
            let base_size = match bf.subtype() {
                Type::Base(base) => base.size() as usize,
                _ => (bf.bit_position() + bf.bit_length()).div_ceil(8) as usize,
            };
            FieldValue::Bitfield {
                bit_position: bf.bit_position(),
                bit_length: bf.bit_length(),
                size: profile.type_size(ty).unwrap_or(0),
                value: read_uint(data, offset, base_size) as u64,
            }
        }
        Type::Enum(eref) => FieldValue::Enum {
            type_name: eref.name(),
            size: profile.type_size(ty).unwrap_or(0),
            value: read_uint(data, offset, 8) as u64,
        },
        Type::Function => FieldValue::Function {
            value: read_uint(data, offset, 8) as u64,
        },
    }
}

fn make_value_kind<'a>(profile: &Profile, ty: Type<'a>) -> FieldValueKind<'a> {
    match &ty {
        Type::Base(base) => match base {
            Base::Void => FieldValueKind::Void,
            Base::Bool => FieldValueKind::Bool,
            Base::Char8 => FieldValueKind::Char8,
            Base::Char16 => FieldValueKind::Char16,
            Base::Char32 => FieldValueKind::Char32,

            Base::I8 => FieldValueKind::I8,
            Base::I16 => FieldValueKind::I16,
            Base::I32 => FieldValueKind::I32,
            Base::I64 => FieldValueKind::I64,
            Base::I128 => FieldValueKind::I128,

            Base::U8 => FieldValueKind::U8,
            Base::U16 => FieldValueKind::U16,
            Base::U32 => FieldValueKind::U32,
            Base::U64 => FieldValueKind::U64,
            Base::U128 => FieldValueKind::U128,
            _ => {
                let size = base.size() as usize;
                match size {
                    1 => FieldValueKind::U8,
                    2 => FieldValueKind::U16,
                    4 => FieldValueKind::U32,
                    8 => FieldValueKind::U64,
                    16 => FieldValueKind::U128,
                    _ => FieldValueKind::Void,
                }
            }
        },
        Type::Struct(sref) => FieldValueKind::Struct {
            type_name: sref.name(),
            size: profile.struct_size(sref.name()).unwrap_or(0),
        },
        Type::Pointer(ptr) => FieldValueKind::Pointer {
            subtype: Box::new(make_value_kind(profile, ptr.subtype())),
            size: profile.type_size(ty).unwrap_or(0),
        },
        Type::Array(arr) => FieldValueKind::Array {
            subtype: Box::new(make_value_kind(profile, arr.subtype())),
            dims: arr.dims().collect(),
            size: profile.type_size(ty).unwrap_or(0),
        },
        Type::Bitfield(bf) => FieldValueKind::Bitfield {
            bit_position: bf.bit_position(),
            bit_length: bf.bit_length(),
            size: profile.type_size(bf.subtype()).unwrap_or(0),
        },
        Type::Enum(eref) => FieldValueKind::Enum {
            type_name: eref.name(),
            size: profile.type_size(ty).unwrap_or(0),
        },
        Type::Function => FieldValueKind::Function,
    }
}

/// Reads a little-endian unsigned integer from a byte slice.
fn read_uint(data: &[u8], offset: usize, size: usize) -> u128 {
    if offset + size > data.len() {
        return 0;
    }

    let slice = &data[offset..offset + size];
    match size {
        1 => slice[0] as u128,
        2 => u16::from_le_bytes(slice.try_into().unwrap()) as u128,
        4 => u32::from_le_bytes(slice.try_into().unwrap()) as u128,
        8 => u64::from_le_bytes(slice.try_into().unwrap()) as u128,
        16 => u128::from_le_bytes(slice.try_into().unwrap()),
        _ => 0,
    }
}
