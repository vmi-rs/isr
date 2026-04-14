use isr_core::schema::{
    Array, Base, Bitfield, Enum, EnumRef, Field, Pointer, Profile, Struct, StructKind, StructRef,
    Type, Variant,
};
use pdb::{
    ClassKind, ClassType, EnumerationType, Error, Indirection, ItemFinder, ItemIter, PointerKind,
    PrimitiveKind, RawString, TypeData, TypeFinder, TypeIndex, UnionType,
};

/// Returns the type name, handling anonymous types.
fn type_name(name: RawString<'_>, index: TypeIndex) -> String {
    let name = String::from_utf8_lossy(name.as_bytes());

    if name.starts_with("<anonymous-")
        || name.starts_with("<unnamed-")
        || name.starts_with("__unnamed")
    {
        return format!("__anonymous_{:x}", u32::from(index));
    }

    name.to_string()
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
    fn parse_types(
        &mut self,
        type_finder: ItemFinder<'p, TypeIndex>,
        type_iter: ItemIter<'p, TypeIndex>,
    ) -> Result<(), Error>;

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

impl<'p> PdbTypes<'p> for Profile {
    fn parse_types(
        &mut self,
        type_finder: ItemFinder<'p, TypeIndex>,
        type_iter: ItemIter<'p, TypeIndex>,
    ) -> Result<(), Error> {
        use pdb::FallibleIterator as _;

        let mut type_finder = type_finder;
        let mut type_iter = type_iter;

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
                    self.add_enum(&type_finder, typ.index(), enumeration)?;
                }

                TypeData::Class(class) if !class.properties.forward_reference() => {
                    self.add_class(&type_finder, typ.index(), class)?;
                }

                TypeData::Union(union) if !union.properties.forward_reference() => {
                    self.add_union(&type_finder, typ.index(), union)?;
                }

                _ => (), // ignore everything else
            }
        }

        Ok(())
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
            tracing::debug!(
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
            tracing::debug!(
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
            tracing::debug!(
                %name,
                new_udt_fields,
                previous_udt_fields = previous_udt.fields.len(),
                "duplicate UDT name; overwriting"
            );
        }

        Ok(())
    }
}

impl<'p> PdbEnum<'p> for Enum {
    fn add_fields(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
    ) -> Result<(), Error> {
        let type_data = match type_finder.find(type_index)?.parse() {
            Ok(data) => data,
            Err(Error::UnimplementedTypeKind(kind)) => {
                tracing::debug!(kind, "skipping unimplemented type kind (enum fields)");
                return Ok(());
            }
            Err(err) => return Err(err),
        };

        match type_data {
            TypeData::FieldList(data) => {
                for field in &data.fields {
                    self.add_field(type_finder, field);
                }

                if let Some(continuation) = data.continuation {
                    self.add_fields(type_finder, continuation)?;
                }
            }

            type_data => {
                tracing::trace!(
                    ?type_index,
                    ?type_data,
                    "ignoring type (expected FieldList)"
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
                tracing::trace!(?type_data, "ignoring type (expected Enumerate)");
            }
        }
    }
}

impl<'p> PdbStruct<'p> for Struct {
    fn add_fields(
        &mut self,
        type_finder: &TypeFinder<'p>,
        type_index: TypeIndex,
    ) -> Result<(), Error> {
        let type_data = match type_finder.find(type_index)?.parse() {
            Ok(data) => data,
            Err(Error::UnimplementedTypeKind(kind)) => {
                tracing::debug!(kind, "skipping unimplemented type kind (struct fields)");
                return Ok(());
            }
            Err(err) => return Err(err),
        };

        match type_data {
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

                if let Some(continuation) = data.continuation
                    && let Err(err) = self.add_fields(type_finder, continuation)
                {
                    tracing::warn!(%err, "failed to parse field");
                }
            }

            type_data => {
                tracing::trace!(
                    ?type_index,
                    ?type_data,
                    "ignoring type (expected FieldList)"
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
                        ty: Type::new(type_finder, data.field_type)?,
                    },
                );
            }

            type_data => {
                tracing::trace!(?type_data, "ignoring type (expected Member)");
            }
        }

        Ok(())
    }
}

impl<'p> PdbType<'p> for Type {
    fn new(type_finder: &TypeFinder<'p>, type_index: TypeIndex) -> Result<Self, Error> {
        let result = match type_finder.find(type_index)?.parse()? {
            TypeData::Primitive(data) => match data.indirection {
                Some(indirection) => Self::Pointer(Pointer {
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
                    tracing::debug!(
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

                Self::Array(Array {
                    subtype: Box::new(Self::new(type_finder, element_type)?),
                    dims: dims.into_iter().rev().collect(),
                })
            }

            TypeData::Pointer(data) => Self::Pointer(Pointer {
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

            TypeData::Bitfield(data) => Self::Bitfield(Bitfield {
                bit_length: data.length as u64,
                bit_position: data.position as u64,
                subtype: Box::new(Self::new(type_finder, data.underlying_type)?),
            }),

            TypeData::Procedure(_) => Self::Function,

            TypeData::Modifier(data) => Self::new(type_finder, data.underlying_type)?,

            type_data => {
                tracing::error!(?type_data, "unknown type");
                Self::Base(Base::Void)
            }
        };

        Ok(result)
    }
}

fn from_primitive_kind(kind: PrimitiveKind) -> Type {
    Type::Base(match kind {
        PrimitiveKind::NoType | PrimitiveKind::Void => Base::Void,

        PrimitiveKind::Bool8
        | PrimitiveKind::Bool16
        | PrimitiveKind::Bool32
        | PrimitiveKind::Bool64 => Base::Bool,

        PrimitiveKind::RChar | PrimitiveKind::Char | PrimitiveKind::UChar => Base::Char8,
        PrimitiveKind::WChar | PrimitiveKind::RChar16 => Base::Char16,
        PrimitiveKind::RChar32 => Base::Char32,

        PrimitiveKind::I8 => Base::I8,
        PrimitiveKind::U8 => Base::U8,
        PrimitiveKind::I16 | PrimitiveKind::Short => Base::I16,
        PrimitiveKind::U16 | PrimitiveKind::UShort => Base::U16,
        PrimitiveKind::I32 | PrimitiveKind::Long | PrimitiveKind::HRESULT => Base::I32,
        PrimitiveKind::U32 | PrimitiveKind::ULong => Base::U32,
        PrimitiveKind::I64 | PrimitiveKind::Quad => Base::I64,
        PrimitiveKind::U64 | PrimitiveKind::UQuad => Base::U64,
        PrimitiveKind::I128 | PrimitiveKind::Octa => Base::I128,
        PrimitiveKind::U128 | PrimitiveKind::UOcta => Base::U128,

        PrimitiveKind::F16 => Base::F16,
        PrimitiveKind::F32 | PrimitiveKind::F32PP => Base::F32,
        PrimitiveKind::F64 => Base::F64,
        PrimitiveKind::F80 | PrimitiveKind::F128 => Base::F128,

        _ => {
            tracing::error!(?kind, "unhandled primitive");
            Base::Void
        }
    })
}
