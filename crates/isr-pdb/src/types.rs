use std::borrow::Cow;

use isr_core::types::{
    ArrayRef, BaseRef, BitfieldRef, Enum, EnumRef, Field, PointerRef, Struct, StructKind,
    StructRef, Type, Types, Variant,
};
use pdb::{
    ClassKind, ClassType, EnumerationType, Error, ItemFinder, ItemIter, PrimitiveKind, RawString,
    TypeData, TypeFinder, TypeIndex, UnionType,
};

fn type_name(name: RawString, index: TypeIndex) -> Cow<'_, str> {
    let name = String::from_utf8_lossy(name.as_bytes());

    if name.starts_with("<anonymous-")
        || name.starts_with("<unnamed-")
        || name.starts_with("__unnamed")
    {
        return Cow::Owned(format!("__anonymous_{:x}", u32::from(index)));
    }

    name
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

/*
impl From<pdb::Variant> for Variant {
    fn from(value: pdb::Variant) -> Self {
        match value {
            pdb::Variant::U8(v) => Self::U8(v),
            pdb::Variant::U16(v) => Self::U16(v),
            pdb::Variant::U32(v) => Self::U32(v),
            pdb::Variant::U64(v) => Self::U64(v),
            pdb::Variant::I8(v) => Self::I8(v),
            pdb::Variant::I16(v) => Self::I16(v),
            pdb::Variant::I32(v) => Self::I32(v),
            pdb::Variant::I64(v) => Self::I64(v),
        }
    }
}
*/

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

            match typ.parse()? {
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
                for field in &data.fields {
                    self.add_field(type_finder, type_index, field)?;
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
                Some(_indirection) => Self::Pointer(PointerRef {
                    subtype: Box::new(from_primitive_kind(data.kind)),
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

            TypeData::Array(data) => Self::Array(ArrayRef {
                subtype: Box::new(Self::new(type_finder, data.element_type)?),
                dims: data.dimensions.iter().map(|dim| *dim as u64).collect(),
                size: data.dimensions.into_iter().product::<u32>() as u64,
            }),

            TypeData::Pointer(data) => Self::Pointer(PointerRef {
                subtype: Box::new(Self::new(type_finder, data.underlying_type)?),
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
        PrimitiveKind::Void => BaseRef::Void,

        PrimitiveKind::Bool8 => BaseRef::Bool,

        PrimitiveKind::RChar => BaseRef::Char,
        PrimitiveKind::Char => BaseRef::Char,
        PrimitiveKind::UChar => BaseRef::Char,
        PrimitiveKind::WChar => BaseRef::Wchar,

        PrimitiveKind::I8 => BaseRef::I8,
        PrimitiveKind::U8 => BaseRef::U8,
        PrimitiveKind::I16 | PrimitiveKind::Short => BaseRef::I16,
        PrimitiveKind::U16 | PrimitiveKind::UShort => BaseRef::U16,
        PrimitiveKind::I32 | PrimitiveKind::Long | PrimitiveKind::HRESULT => BaseRef::I32,
        PrimitiveKind::U32 | PrimitiveKind::ULong => BaseRef::U32,
        PrimitiveKind::I64 | PrimitiveKind::Quad => BaseRef::I64,
        PrimitiveKind::U64 | PrimitiveKind::UQuad => BaseRef::U64,

        PrimitiveKind::F32 => BaseRef::F32,
        PrimitiveKind::F64 => BaseRef::F64,

        _ => {
            tracing::error!(?kind, "Unhandled primitive");
            BaseRef::Void
        }
    })
}
