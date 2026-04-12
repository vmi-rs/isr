use std::borrow::Cow;

use isr_core::types::{
    ArrayRef, BaseRef, BitfieldRef, Enum, EnumRef, Field, PointerRef, Struct, StructKind,
    StructRef, Type, Types, Variant,
};
use pdb::{
    ClassKind, ClassType, EnumerationType, Error, Indirection, ItemFinder, ItemIter, PointerKind,
    PrimitiveKind, RawString, TypeData, TypeFinder, TypeIndex, UnionType,
};

/// Returns the type name, handling anonymous types.
fn type_name(name: RawString<'_>, index: TypeIndex) -> Cow<'_, str> {
    let name = String::from_utf8_lossy(name.as_bytes());

    if name.starts_with("<anonymous-")
        || name.starts_with("<unnamed-")
        || name.starts_with("__unnamed")
    {
        return Cow::Owned(format!("__anonymous_{:x}", u32::from(index)));
    }

    name
}

/// Returns the size of a type in bytes.
fn type_size(type_finder: &TypeFinder, type_index: TypeIndex) -> Result<u64, Error> {
    let size = match type_finder.find(type_index)?.parse()? {
        TypeData::Primitive(data) => {
            let mut size = match data.kind {
                PrimitiveKind::Char
                | PrimitiveKind::RChar
                | PrimitiveKind::UChar
                | PrimitiveKind::I8
                | PrimitiveKind::U8
                | PrimitiveKind::Bool8 => 1,

                PrimitiveKind::WChar
                | PrimitiveKind::RChar16
                | PrimitiveKind::I16
                | PrimitiveKind::Short
                | PrimitiveKind::U16
                | PrimitiveKind::UShort
                | PrimitiveKind::F16
                | PrimitiveKind::Bool16 => 2,

                PrimitiveKind::RChar32
                | PrimitiveKind::I32
                | PrimitiveKind::Long
                | PrimitiveKind::U32
                | PrimitiveKind::ULong
                | PrimitiveKind::F32
                | PrimitiveKind::F32PP
                | PrimitiveKind::HRESULT
                | PrimitiveKind::Bool32 => 4,

                PrimitiveKind::I64
                | PrimitiveKind::Quad
                | PrimitiveKind::U64
                | PrimitiveKind::UQuad
                | PrimitiveKind::F64
                | PrimitiveKind::Bool64 => 8,

                PrimitiveKind::I128
                | PrimitiveKind::U128
                | PrimitiveKind::Octa
                | PrimitiveKind::UOcta
                | PrimitiveKind::F80
                | PrimitiveKind::F128 => 16,

                _ => 0,
            };

            if let Some(indirection) = data.indirection {
                size = match indirection {
                    Indirection::Near16 | Indirection::Far16 | Indirection::Huge16 => 2,
                    Indirection::Near32 | Indirection::Far32 => 4,
                    Indirection::Near64 => 8,
                    Indirection::Near128 => 16,
                };
            }

            size
        }
        TypeData::Class(data) => data.size,
        TypeData::Enumeration(data) => type_size(type_finder, data.underlying_type)?,
        TypeData::Union(data) => data.size,
        TypeData::Pointer(data) => match data.attributes.pointer_kind() {
            PointerKind::Near16 | PointerKind::Far16 | PointerKind::Huge16 => 2,
            PointerKind::Near32
            | PointerKind::Far32
            | PointerKind::BaseSeg
            | PointerKind::BaseVal
            | PointerKind::BaseSegVal
            | PointerKind::BaseAddr
            | PointerKind::BaseSegAddr
            | PointerKind::BaseType
            | PointerKind::BaseSelf => 4,
            PointerKind::Ptr64 => 8,
        },
        TypeData::Modifier(data) => type_size(type_finder, data.underlying_type)?,
        TypeData::Array(data) => data.dimensions.into_iter().last().unwrap_or(0) as u64,
        _ => 0,
    };

    Ok(size)
}

pub trait PdbTypes<'p>
where
    Self: Sized,
{
    fn parse(
        type_finder: ItemFinder<'p, TypeIndex>,
        type_iter: ItemIter<'p, TypeIndex>,
    ) -> Result<Self, Error>;

    fn add_enum(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
        enumeration: EnumerationType<'p>,
    ) -> Result<(), Error>;

    fn add_class(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
        class: ClassType<'p>,
    ) -> Result<(), Error>;

    fn add_union(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
        union: UnionType<'p>,
    ) -> Result<(), Error>;
}

trait PdbEnum<'p> {
    fn add_fields(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
    ) -> Result<(), Error>;

    fn add_field(&mut self, type_finder: &TypeFinder<'p>, field: &TypeData<'p>);
}

trait PdbStruct<'p> {
    fn add_fields(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
    ) -> Result<(), Error>;

    fn add_field(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
        field: &TypeData<'p>,
    ) -> Result<(), Error>;
}

trait PdbType<'p>
where
    Self: Sized,
{
    fn new(type_finder: &TypeFinder<'p>, type_index: TypeIndex) -> Result<Self, Error>;
}

/// Converts a `pdb::Variant` to a `Variant`.
fn convert_variant(variant: pdb::Variant) -> Variant {
    match variant {
        pdb::Variant::U8(value) => Variant::U8(value),
        pdb::Variant::U16(value) => Variant::U16(value),
        pdb::Variant::U32(value) => Variant::U32(value),
        pdb::Variant::U64(value) => Variant::U64(value),
        pdb::Variant::I8(value) => Variant::I8(value),
        pdb::Variant::I16(value) => Variant::I16(value),
        pdb::Variant::I32(value) => Variant::I32(value),
        pdb::Variant::I64(value) => Variant::I64(value),
    }
}

impl<'p> PdbTypes<'p> for Types<'p> {
    fn parse(
        type_finder: ItemFinder<'p, TypeIndex>,
        type_iter: ItemIter<'p, TypeIndex>,
    ) -> Result<Self, Error> {
        use pdb::FallibleIterator as _;

        let mut type_finder = type_finder;
        let mut type_iter = type_iter;

        let mut result = Self::default();

        while let Some(typ) = type_iter.next()? {
            // keep building the index
            type_finder.update(&type_iter);

            let type_data = match typ.parse() {
                Ok(data) => data,
                Err(Error::UnimplementedTypeKind(kind)) => {
                    tracing::debug!(kind, "skipping unimplemented type kind");
                    continue;
                }
                Err(err) => return Err(err),
            };

            match type_data {
                TypeData::Enumeration(enumeration)
                    if !enumeration.properties.forward_reference() =>
                {
                    result.add_enum(&type_finder, typ.index(), enumeration)?;
                }

                TypeData::Class(class) if !class.properties.forward_reference() => {
                    result.add_class(&type_finder, typ.index(), class)?;
                }

                TypeData::Union(union) if !union.properties.forward_reference() => {
                    result.add_union(&type_finder, typ.index(), union)?;
                }

                _ => (), // ignore everything else
            }
        }

        Ok(result)
    }

    fn add_enum(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
        enumeration: EnumerationType<'p>,
    ) -> Result<(), Error> {
        let name = type_name(enumeration.name, type_index);

        let mut new_enum = Enum {
            subtype: Type::new(type_finder, enumeration.underlying_type)?,
            fields: Default::default(),
        };

        new_enum.add_fields(type_finder, enumeration.fields)?;

        let new_enum_fields = new_enum.fields.len();

        if let Some(previous_udt) = self.enums.insert(name.clone(), new_enum) {
            tracing::warn!(
                %name,
                new_enum_fields,
                previous_enum_fields = previous_udt.fields.len(),
                "duplicate enum name; overwriting"
            );
        }

        Ok(())
    }

    fn add_class(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
        class: ClassType<'p>,
    ) -> Result<(), Error> {
        let name = type_name(class.name, type_index);

        let mut new_udt = Struct {
            kind: match class.kind {
                ClassKind::Struct => StructKind::Struct,
                ClassKind::Class => StructKind::Class,
                ClassKind::Interface => StructKind::Interface,
            },
            size: class.size,
            fields: Default::default(),
        };

        if let Some(fields) = class.fields {
            new_udt.add_fields(type_finder, fields)?;
        }

        let new_udt_fields = new_udt.fields.len();

        if let Some(previous_udt) = self.structs.insert(name.clone(), new_udt) {
            tracing::warn!(
                %name,
                new_udt_fields,
                previous_udt_fields = previous_udt.fields.len(),
                "duplicate UDT name; overwriting"
            );
        }

        Ok(())
    }

    fn add_union(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
        union: UnionType<'p>,
    ) -> Result<(), Error> {
        let name = type_name(union.name, type_index);

        let mut new_udt = Struct {
            kind: StructKind::Union,
            size: union.size,
            fields: Default::default(),
        };

        new_udt.add_fields(type_finder, union.fields)?;

        let new_udt_fields = new_udt.fields.len();

        if let Some(previous_udt) = self.structs.insert(name.clone(), new_udt) {
            tracing::warn!(
                %name,
                new_udt_fields,
                previous_udt_fields = previous_udt.fields.len(),
                "duplicate UDT name; overwriting"
            );
        }

        Ok(())
    }
}

impl<'p> PdbEnum<'p> for Enum<'p> {
    fn add_fields(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
    ) -> Result<(), Error> {
        match type_finder.find(type_index)?.parse()? {
            TypeData::FieldList(data) => {
                for field in &data.fields {
                    self.add_field(type_finder, field);
                }

                if let Some(continuation) = data.continuation {
                    self.add_fields(type_finder, continuation)?;
                }
            }

            type_data => {
                tracing::warn!(
                    ?type_index,
                    ?type_data,
                    "unexpected type (expected FieldList)"
                );
            }
        }

        Ok(())
    }

    fn add_field(&mut self, _type_finder: &TypeFinder<'p>, field: &TypeData<'p>) {
        match field {
            TypeData::Enumerate(data) => {
                let name = match std::str::from_utf8(data.name.as_bytes()) {
                    Ok(name) => name,
                    Err(_) => {
                        tracing::warn!(name = %data.name, "failed to convert field name to UTF-8");
                        return;
                    }
                };

                self.fields.insert(name.into(), convert_variant(data.value));
            }

            type_data => {
                tracing::warn!(?type_data, "unexpected type (expected Enumerate)");
            }
        }
    }
}

impl<'p> PdbStruct<'p> for Struct<'p> {
    fn add_fields(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
    ) -> Result<(), Error> {
        match type_finder.find(type_index)?.parse()? {
            TypeData::FieldList(data) => {
                // We don't use self.add_field()? because the pdb crate doesn't
                // recognize the char8_t type and thus fails to parse some
                // fields.
                //
                // https://github.com/getsentry/pdb/issues/130

                for field in &data.fields {
                    if let Err(err) = self.add_field(type_finder, type_index, field) {
                        tracing::warn!(%err, "failed to parse field");
                    }
                }

                if let Some(continuation) = data.continuation {
                    if let Err(err) = self.add_fields(type_finder, continuation) {
                        tracing::warn!(%err, "failed to parse field");
                    }
                }
            }

            type_data => {
                tracing::warn!(
                    ?type_index,
                    ?type_data,
                    "unexpected type (expected FieldList)"
                );
            }
        }

        Ok(())
    }

    fn add_field(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
        field: &TypeData<'p>,
    ) -> Result<(), Error> {
        match field {
            TypeData::Member(data) => {
                self.fields.insert(
                    type_name(data.name, type_index),
                    Field {
                        offset: data.offset,
                        type_: Type::new(type_finder, data.field_type)?,
                    },
                );
            }

            type_data => {
                tracing::warn!(?type_data, "unexpected type (expected Member)");
            }
        }

        Ok(())
    }
}

impl<'p> PdbType<'p> for Type<'p> {
    fn new(type_finder: &TypeFinder<'p>, type_index: TypeIndex) -> Result<Self, Error> {
        let result = match type_finder.find(type_index)?.parse()? {
            TypeData::Primitive(data) => match data.indirection {
                Some(indirection) => Self::Pointer(PointerRef {
                    subtype: Box::new(from_primitive_kind(data.kind)),
                    size: match indirection {
                        Indirection::Near16 | Indirection::Far16 | Indirection::Huge16 => 2,
                        Indirection::Near32 | Indirection::Far32 => 4,
                        Indirection::Near64 => 8,
                        Indirection::Near128 => 16,
                    },
                }),
                None => from_primitive_kind(data.kind),
            },

            TypeData::Enumeration(data) => Self::Enum(EnumRef {
                name: type_name(data.name, type_index),
            }),

            TypeData::Union(data) => Self::Struct(StructRef {
                name: type_name(data.name, type_index),
            }),

            TypeData::Class(data) => Self::Struct(StructRef {
                name: type_name(data.name, type_index),
            }),

            TypeData::Array(data) => {
                let mut dimensions = data.dimensions;
                let mut element_type = data.element_type;
                while let TypeData::Array(array) = type_finder.find(element_type)?.parse()? {
                    dimensions.extend(array.dimensions);
                    element_type = array.element_type;
                }

                let element_type_size = type_size(type_finder, element_type)?;
                if element_type_size == 0 {
                    tracing::warn!(
                        ?type_index,
                        "element type has size 0, dimensions may be incorrect"
                    );
                }

                let mut divider = element_type_size.max(1); // prevent division by zero

                let dims = dimensions
                    .into_iter()
                    .rev()
                    .map(|dim_size| {
                        let dim_size = dim_size as u64;
                        let result = dim_size / divider;
                        divider = dim_size;
                        result
                    })
                    .collect::<Vec<_>>();

                Self::Array(ArrayRef {
                    subtype: Box::new(Self::new(type_finder, element_type)?),
                    dims: dims.into_iter().rev().collect(),
                })
            }

            TypeData::Pointer(data) => Self::Pointer(PointerRef {
                subtype: Box::new(Self::new(type_finder, data.underlying_type)?),
                size: match data.attributes.pointer_kind() {
                    PointerKind::Near16 | PointerKind::Far16 | PointerKind::Huge16 => 2,

                    PointerKind::Near32
                    | PointerKind::Far32
                    | PointerKind::BaseSeg
                    | PointerKind::BaseVal
                    | PointerKind::BaseSegVal
                    | PointerKind::BaseAddr
                    | PointerKind::BaseSegAddr
                    | PointerKind::BaseType
                    | PointerKind::BaseSelf => 4,

                    PointerKind::Ptr64 => 8,
                },
            }),

            TypeData::Bitfield(data) => Self::Bitfield(BitfieldRef {
                bit_length: data.length as u64,
                bit_position: data.position as u64,
                subtype: Box::new(Self::new(type_finder, data.underlying_type)?),
            }),

            TypeData::Procedure(_) => Self::Function,

            TypeData::Modifier(data) => Self::new(type_finder, data.underlying_type)?,

            type_data => {
                tracing::error!(?type_data, "Unknown type");
                Self::Base(BaseRef::Void)
            }
        };

        Ok(result)
    }
}

fn from_primitive_kind<'p>(kind: PrimitiveKind) -> Type<'p> {
    Type::Base(match kind {
        PrimitiveKind::NoType | PrimitiveKind::Void => BaseRef::Void,

        PrimitiveKind::Bool8 | PrimitiveKind::Bool16 | PrimitiveKind::Bool32
        | PrimitiveKind::Bool64 => BaseRef::Bool,

        PrimitiveKind::RChar | PrimitiveKind::Char | PrimitiveKind::UChar => BaseRef::Char,
        PrimitiveKind::WChar => BaseRef::Wchar,
        PrimitiveKind::RChar16 => BaseRef::U16,
        PrimitiveKind::RChar32 => BaseRef::U32,

        PrimitiveKind::I8 => BaseRef::I8,
        PrimitiveKind::U8 => BaseRef::U8,
        PrimitiveKind::I16 | PrimitiveKind::Short => BaseRef::I16,
        PrimitiveKind::U16 | PrimitiveKind::UShort => BaseRef::U16,
        PrimitiveKind::I32 | PrimitiveKind::Long | PrimitiveKind::HRESULT => BaseRef::I32,
        PrimitiveKind::U32 | PrimitiveKind::ULong => BaseRef::U32,
        PrimitiveKind::I64 | PrimitiveKind::Quad => BaseRef::I64,
        PrimitiveKind::U64 | PrimitiveKind::UQuad => BaseRef::U64,
        PrimitiveKind::I128 | PrimitiveKind::Octa => BaseRef::I128,
        PrimitiveKind::U128 | PrimitiveKind::UOcta => BaseRef::U128,

        PrimitiveKind::F16 => BaseRef::F16,
        PrimitiveKind::F32 | PrimitiveKind::F32PP => BaseRef::F32,
        PrimitiveKind::F64 => BaseRef::F64,
        PrimitiveKind::F80 | PrimitiveKind::F128 => BaseRef::F128,

        _ => {
            tracing::error!(?kind, "Unhandled primitive");
            BaseRef::Void
        }
    })
}
