//! ISR core library.

pub mod schema;
pub mod visit;

use crate::schema::{
    ArchivedArchitecture, ArchivedArray, ArchivedBase, ArchivedBitfield, ArchivedEnum,
    ArchivedEnumRef, ArchivedField, ArchivedPointer, ArchivedProfile, ArchivedStruct,
    ArchivedStructKind, ArchivedStructRef, ArchivedType, ArchivedVariant,
};

/// A typed view over an archived profile.
pub struct Profile<'a> {
    inner: &'a ArchivedProfile,
    rva_to_symbol: Vec<Symbol<'a>>,
}

/// A named symbol resolved to its relative virtual address.
#[derive(Debug, Clone, Copy)]
pub struct Symbol<'a> {
    /// Symbol name.
    pub name: &'a str,

    /// Relative virtual address from the module base.
    pub rva: u64,
}

impl<'a> Profile<'a> {
    /// Constructs a typed view over an archived profile.
    pub fn from_archived(archived: &'a ArchivedProfile) -> Self {
        let mut rva_to_symbol = archived
            .symbols
            .iter()
            .map(|(name, &rva)| Symbol {
                name: name.as_ref(),
                rva: rva.into(),
            })
            .collect::<Vec<_>>();
        rva_to_symbol.sort_unstable_by_key(|entry| entry.rva);

        Self {
            inner: archived,
            rva_to_symbol,
        }
    }

    /// Returns the architecture of the profile.
    pub fn architecture(&self) -> Architecture {
        Architecture::from_archived(&self.inner.architecture)
    }

    /// Returns the enums.
    pub fn enums(&self) -> impl ExactSizeIterator<Item = Enum<'a>> {
        self.inner.enums.iter().map(|(name, inner)| Enum {
            name: name.as_ref(),
            inner,
        })
    }

    /// Returns the structs.
    pub fn structs(&self) -> impl ExactSizeIterator<Item = Struct<'a>> {
        self.inner.structs.iter().map(|(name, inner)| Struct {
            name: name.as_ref(),
            inner,
        })
    }

    /// Returns the symbols.
    pub fn symbols(&self) -> impl ExactSizeIterator<Item = Symbol<'a>> {
        self.inner.symbols.iter().map(|(name, &rva)| Symbol {
            name: name.as_ref(),
            rva: rva.into(),
        })
    }
    /// Returns the size of a given type in bytes.
    pub fn type_size(&self, ty: Type<'a>) -> Option<u64> {
        match ty {
            Type::Base(v) => Some(v.size()),
            Type::Enum(v) => self.enum_size(v.name()),
            Type::Struct(v) => self.struct_size(v.name()),
            Type::Array(v) => self
                .type_size(v.subtype())
                .map(|subtype_size| subtype_size * v.dims().product::<u64>()),
            Type::Pointer(v) => Some(v.size()),
            Type::Bitfield(v) => self.type_size(v.subtype()),
            Type::Function => Some(self.pointer_size()),
        }
    }

    /// Returns the size of an enum type in bytes.
    pub fn enum_size(&self, name: &str) -> Option<u64> {
        let ty = Type::from_archived(&self.inner.enums.get(name)?.subtype);
        self.type_size(ty)
    }

    /// Returns the size of a struct type in bytes.
    pub fn struct_size(&self, name: &str) -> Option<u64> {
        self.inner.structs.get(name).map(|udt| udt.size.into())
    }

    /// Returns the size of a pointer in bytes.
    pub fn pointer_size(&self) -> u64 {
        match self.architecture() {
            Architecture::X86 | Architecture::Arm32 => 4,
            Architecture::Amd64 | Architecture::Arm64 => 8,
            Architecture::Unknown => 0,
        }
    }

    /// Finds an enum by name.
    pub fn find_enum(&self, type_name: &str) -> Option<Enum<'a>> {
        self.inner
            .enums
            .get_key_value(type_name)
            .map(|(name, inner)| Enum {
                name: name.as_ref(),
                inner,
            })
    }

    /// Finds a struct by name.
    pub fn find_struct(&self, type_name: &str) -> Option<Struct<'a>> {
        self.inner
            .structs
            .get_key_value(type_name)
            .map(|(name, inner)| Struct {
                name: name.as_ref(),
                inner,
            })
    }

    /// Finds a symbol by name.
    pub fn find_symbol(&self, symbol_name: &str) -> Option<u64> {
        self.inner.symbols.get(symbol_name).map(Into::into)
    }

    /// Returns the symbol whose RVA is closest to `rva` without exceeding it.
    ///
    /// Useful for resolving an address to the containing function.
    pub fn lookup_symbol(&self, rva: u64) -> Option<Symbol<'a>> {
        let idx = match self.rva_to_symbol.binary_search_by_key(&rva, |e| e.rva) {
            Ok(i) => i,
            Err(0) => return None,
            Err(i) => i - 1,
        };

        Some(self.rva_to_symbol[idx])
    }
}

/// Target CPU architecture.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Architecture {
    Unknown,
    X86,
    Amd64,
    Arm32,
    Arm64,
}

impl Architecture {
    fn from_archived(value: &ArchivedArchitecture) -> Self {
        match value {
            ArchivedArchitecture::Unknown => Self::Unknown,
            ArchivedArchitecture::X86 => Self::X86,
            ArchivedArchitecture::Amd64 => Self::Amd64,
            ArchivedArchitecture::Arm32 => Self::Arm32,
            ArchivedArchitecture::Arm64 => Self::Arm64,
        }
    }
}

/// A named enum type in a profile.
#[derive(Debug, Clone, Copy)]
pub struct Enum<'a> {
    name: &'a str,
    inner: &'a ArchivedEnum,
}

impl<'a> Enum<'a> {
    /// Returns the name of the enum.
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// Returns the underlying integer type of the enum.
    pub fn subtype(&self) -> Type<'a> {
        Type::from_archived(&self.inner.subtype)
    }

    /// Returns the variants of the enum with their discriminant values.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = (&'a str, Variant)> {
        self.inner
            .fields
            .iter()
            .map(|(name, variant)| (name.as_ref(), Variant::from_archived(variant)))
    }
}

/// Discriminant value of an enum variant.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl Variant {
    fn from_archived(value: &ArchivedVariant) -> Self {
        match value {
            ArchivedVariant::U8(v) => Self::U8(*v),
            ArchivedVariant::U16(v) => Self::U16(v.into()),
            ArchivedVariant::U32(v) => Self::U32(v.into()),
            ArchivedVariant::U64(v) => Self::U64(v.into()),
            ArchivedVariant::U128(v) => Self::U128(v.into()),
            ArchivedVariant::I8(v) => Self::I8(*v),
            ArchivedVariant::I16(v) => Self::I16(v.into()),
            ArchivedVariant::I32(v) => Self::I32(v.into()),
            ArchivedVariant::I64(v) => Self::I64(v.into()),
            ArchivedVariant::I128(v) => Self::I128(v.into()),
        }
    }
}

/// Struct type.
#[derive(Debug, Clone, Copy)]
pub struct Struct<'a> {
    name: &'a str,
    inner: &'a ArchivedStruct,
}

impl<'a> Struct<'a> {
    /// Returns the name of the struct.
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// Returns the kind of the struct.
    pub fn kind(&self) -> StructKind {
        StructKind::from_archived(&self.inner.kind)
    }

    /// Returns the size of the struct in bytes.
    pub fn size(&self) -> u64 {
        self.inner.size.into()
    }

    /// Finds a field by name.
    pub fn field(&self, name: &str) -> Option<Field<'a>> {
        self.inner
            .fields
            .get_key_value(name)
            .map(|(name, field)| Field {
                name: name.as_ref(),
                inner: field,
            })
    }

    /// Returns the fields of the struct.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = Field<'a>> {
        self.inner.fields.iter().map(|(name, inner)| Field {
            name: name.as_ref(),
            inner,
        })
    }
}

/// Struct kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

impl StructKind {
    fn from_archived(value: &ArchivedStructKind) -> Self {
        match value {
            ArchivedStructKind::Struct => Self::Struct,
            ArchivedStructKind::Class => Self::Class,
            ArchivedStructKind::Union => Self::Union,
            ArchivedStructKind::Interface => Self::Interface,
        }
    }
}

/// Struct field.
#[derive(Debug, Clone, Copy)]
pub struct Field<'a> {
    name: &'a str,
    inner: &'a ArchivedField,
}

impl<'a> Field<'a> {
    /// Returns the name of the field.
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// Returns the offset of the field in bytes.
    pub fn offset(&self) -> u64 {
        self.inner.offset.into()
    }

    /// Returns the type of the field.
    pub fn ty(&self) -> Type<'a> {
        Type::from_archived(&self.inner.ty)
    }
}

/// Type.
#[derive(Debug, Clone, Copy)]
pub enum Type<'a> {
    /// Base type.
    Base(Base),

    /// Enum type.
    Enum(EnumRef<'a>),

    /// Struct type.
    Struct(StructRef<'a>),

    /// Array type.
    Array(Array<'a>),

    /// Pointer type.
    Pointer(Pointer<'a>),

    /// Bitfield type.
    Bitfield(Bitfield<'a>),

    /// Function type.
    Function,
}

impl<'a> Type<'a> {
    fn from_archived(value: &'a ArchivedType) -> Self {
        match value {
            ArchivedType::Base(inner) => Self::Base(Base::from_archived(inner)),
            ArchivedType::Enum(inner) => Self::Enum(EnumRef { inner }),
            ArchivedType::Struct(inner) => Self::Struct(StructRef { inner }),
            ArchivedType::Array(inner) => Self::Array(Array { inner }),
            ArchivedType::Pointer(inner) => Self::Pointer(Pointer { inner }),
            ArchivedType::Bitfield(inner) => Self::Bitfield(Bitfield { inner }),
            ArchivedType::Function => Self::Function,
        }
    }
}

/// Base type.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    fn from_archived(value: &ArchivedBase) -> Self {
        match value {
            ArchivedBase::Void => Self::Void,

            ArchivedBase::Bool => Self::Bool,

            ArchivedBase::Char8 => Self::Char8,
            ArchivedBase::Char16 => Self::Char16,
            ArchivedBase::Char32 => Self::Char32,

            ArchivedBase::I8 => Self::I8,
            ArchivedBase::I16 => Self::I16,
            ArchivedBase::I32 => Self::I32,
            ArchivedBase::I64 => Self::I64,
            ArchivedBase::I128 => Self::I128,

            ArchivedBase::U8 => Self::U8,
            ArchivedBase::U16 => Self::U16,
            ArchivedBase::U32 => Self::U32,
            ArchivedBase::U64 => Self::U64,
            ArchivedBase::U128 => Self::U128,

            ArchivedBase::F8 => Self::F8,
            ArchivedBase::F16 => Self::F16,
            ArchivedBase::F32 => Self::F32,
            ArchivedBase::F64 => Self::F64,
            ArchivedBase::F128 => Self::F128,
        }
    }

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

/// An enum reference.
#[derive(Debug, Clone, Copy)]
pub struct EnumRef<'a> {
    inner: &'a ArchivedEnumRef,
}

impl<'a> EnumRef<'a> {
    /// Returns the name of the enum.
    pub fn name(&self) -> &'a str {
        &self.inner.name
    }
}

/// A struct reference.
#[derive(Debug, Clone, Copy)]
pub struct StructRef<'a> {
    inner: &'a ArchivedStructRef,
}

impl<'a> StructRef<'a> {
    /// Returns the name of the struct.
    pub fn name(&self) -> &'a str {
        &self.inner.name
    }
}

/// Array type.
#[derive(Debug, Clone, Copy)]
pub struct Array<'a> {
    inner: &'a ArchivedArray,
}

impl<'a> Array<'a> {
    /// Returns the subtype of the array.
    pub fn subtype(&self) -> Type<'a> {
        Type::from_archived(self.inner.subtype.as_ref())
    }

    /// Returns the dimensions of the array.
    pub fn dims(&self) -> impl Iterator<Item = u64> + use<'a> {
        self.inner.dims.iter().map(|&dim| dim.to_native())
    }
}

/// Bitfield type.
#[derive(Debug, Clone, Copy)]
pub struct Bitfield<'a> {
    inner: &'a ArchivedBitfield,
}

impl<'a> Bitfield<'a> {
    /// Returns the subtype of the bitfield.
    pub fn subtype(&self) -> Type<'a> {
        Type::from_archived(self.inner.subtype.as_ref())
    }

    /// Returns the length of the bitfield in bits.
    pub fn bit_length(&self) -> u64 {
        self.inner.bit_length.into()
    }

    /// Returns the starting bit position within the subtype.
    pub fn bit_position(&self) -> u64 {
        self.inner.bit_position.into()
    }
}

/// Pointer type.
#[derive(Debug, Clone, Copy)]
pub struct Pointer<'a> {
    inner: &'a ArchivedPointer,
}

impl<'a> Pointer<'a> {
    /// Returns the subtype of the pointer.
    pub fn subtype(&self) -> Type<'a> {
        Type::from_archived(self.inner.subtype.as_ref())
    }

    /// Returns the size of the pointer in bytes.
    pub fn size(&self) -> u64 {
        self.inner.size.into()
    }
}
