use std::borrow::Cow;

use gimli::{
    Attribute, DebuggingInformationEntry, EntriesTree, EntriesTreeNode, Error, Reader as _,
    UnitRef, UnitSectionOffset,
};
use indexmap::map::Entry;
use isr_core::types::{
    ArrayRef, BaseRef, BitfieldRef, Enum, EnumRef, Field, PointerRef, Struct, StructKind,
    StructRef, Type, Types, Variant,
};
use smallvec::SmallVec;

use super::_gimli::{DebuggingInformationEntryExt as _, Reader};

fn type_name<'data>(
    unit: &UnitRef<Reader<'data>>,
    entry: &DebuggingInformationEntry<Reader<'data>>,
) -> Result<Cow<'data, str>, Error> {
    match entry.name(unit)? {
        Some(name) => Ok(name.into()),
        None => {
            let offset = match entry.offset().to_unit_section_offset(unit) {
                UnitSectionOffset::DebugInfoOffset(offset) => offset.0,
                UnitSectionOffset::DebugTypesOffset(offset) => offset.0,
            };

            Ok(format!("__unnamed_{:x}", offset).into())
        }
    }
}

pub type DwarfCache = std::collections::HashSet<(String, u64, u64)>;

pub trait DwarfTypes<'data>
where
    Self: Sized,
{
    fn add(&mut self, unit: &UnitRef<Reader<'data>>, cache: &mut DwarfCache) -> Result<(), Error>;

    fn add_enum(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<(), Error>;

    fn add_struct(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
        kind: StructKind,
    ) -> Result<(), Error>;
}

trait DwarfStruct<'data> {
    fn add_fields(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<(), Error>;

    fn add_field(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<(), Error>;
}

trait DwarfEnum<'data> {
    fn add_fields(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<(), Error>;

    fn add_field(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<(), Error>;
}

trait DwarfType<'data>
where
    Self: Sized,
{
    fn new(
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<Self, Error>;

    fn from_type(
        unit: &UnitRef<Reader<'data>>,
        type_: EntriesTree<Reader<'data>>,
    ) -> Result<Self, Error>;
}

impl<'data> DwarfTypes<'data> for Types<'data> {
    fn add(&mut self, unit: &UnitRef<Reader<'data>>, cache: &mut DwarfCache) -> Result<(), Error> {
        let mut tree = unit.entries_tree(None)?;
        let mut children = tree.root()?.children();

        while let Some(child) = children.next()? {
            if !matches!(
                child.entry().tag(),
                gimli::DW_TAG_enumeration_type
                    | gimli::DW_TAG_structure_type
                    | gimli::DW_TAG_union_type
            ) {
                continue;
            }

            if child.entry().declaration()?.unwrap_or(false) {
                continue;
            }

            let decl_file = child.entry().decl_file(unit)?;
            let decl_line = child.entry().decl_line()?;
            let decl_column = child.entry().decl_column()?;

            match (decl_file, decl_line, decl_column) {
                (Some(decl_file), Some(decl_line), Some(decl_column)) => {
                    if !cache.insert((decl_file, decl_line, decl_column)) {
                        continue;
                    }
                }
                _ => {
                    let name = type_name(unit, child.entry())?;
                    tracing::warn!(%name, "missing declaration information");
                }
            }

            match child.entry().tag() {
                gimli::DW_TAG_enumeration_type => self.add_enum(unit, child)?,
                gimli::DW_TAG_structure_type => self.add_struct(unit, child, StructKind::Struct)?,
                gimli::DW_TAG_union_type => self.add_struct(unit, child, StructKind::Union)?,

                // Skip other tags.
                _ => (),
            }
        }

        Ok(())
    }

    #[tracing::instrument(skip_all, err, fields(name))]
    fn add_enum(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<(), Error> {
        let name = type_name(unit, node.entry())?;
        tracing::Span::current().record("name", &*name);

        let type_ = match node.entry().type_(unit)? {
            Some(type_) => type_,
            None => {
                tracing::warn!("enum doesn't have a type");
                return Ok(());
            }
        };

        let mut new_enum = Enum {
            subtype: Type::from_type(unit, type_)?,
            fields: Default::default(),
        };

        new_enum.add_fields(unit, node)?;

        let new_enum_fields = new_enum.fields.len();

        match self.enums.entry(name.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(new_enum);
            }
            Entry::Occupied(mut entry) => {
                let previous_udt = entry.get_mut();
                let previous_enum_fields = previous_udt.fields.len();

                if new_enum_fields > previous_enum_fields {
                    tracing::warn!(
                        %name,
                        new_enum_fields,
                        previous_enum_fields,
                        "duplicate enum name; overwriting"
                    );

                    *previous_udt = new_enum;
                }
            }
        }

        Ok(())
    }

    #[tracing::instrument(skip_all, err, fields(name))]
    fn add_struct(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
        kind: StructKind,
    ) -> Result<(), Error> {
        let name = type_name(unit, node.entry())?;
        tracing::Span::current().record("name", &*name);

        let mut new_udt = Struct {
            kind,
            size: node.entry().byte_size()?.unwrap_or(0),
            fields: Default::default(),
        };

        new_udt.add_fields(unit, node)?;

        let new_udt_fields = new_udt.fields.len();

        match self.structs.entry(name.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(new_udt);
            }
            Entry::Occupied(mut entry) => {
                let previous_udt = entry.get_mut();
                let previous_udt_fields = previous_udt.fields.len();

                if new_udt_fields > previous_udt_fields {
                    tracing::warn!(
                        %name,
                        new_udt_fields,
                        previous_udt_fields,
                        "duplicate UDT name; overwriting"
                    );

                    *previous_udt = new_udt;
                }
            }
        }

        Ok(())
    }
}

impl<'data> DwarfStruct<'data> for Struct<'data> {
    fn add_fields(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<(), Error> {
        let mut children = node.children();

        while let Some(child) = children.next()? {
            if child.entry().tag() != gimli::DW_TAG_member {
                tracing::warn!(
                    tag = ?child.entry().tag(),
                    "unexpected tag (expected DW_TAG_member)"
                );

                continue;
            }

            self.add_field(unit, child)?;
        }

        Ok(())
    }

    #[tracing::instrument(skip_all, err, fields(name))]
    fn add_field(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<(), Error> {
        debug_assert_eq!(node.entry().tag(), gimli::DW_TAG_member);

        let name = match node.entry().name(unit)? {
            Some(name) => name,
            None => format!("__unnamed_field_{:x}", self.fields.len()),
        };
        tracing::Span::current().record("name", &name);

        let offset = match node.entry().data_member_location()? {
            Some(offset) => offset,
            None => match node.entry().data_bit_offset()? {
                Some(bit_offset) => bit_offset / 8,
                // Assume zero offset if no offset is found.
                None => 0,
            },
        };

        self.fields.insert(
            name.into(),
            Field {
                offset,
                type_: Type::new(unit, node)?,
            },
        );

        Ok(())
    }
}

impl<'data> DwarfEnum<'data> for Enum<'data> {
    fn add_fields(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<(), Error> {
        let mut children = node.children();

        while let Some(child) = children.next()? {
            if child.entry().tag() != gimli::DW_TAG_enumerator {
                tracing::warn!(
                    tag = ?child.entry().tag(),
                    "unexpected tag (expected DW_TAG_enumerator)"
                );

                continue;
            }

            self.add_field(unit, child)?;
        }

        Ok(())
    }

    #[tracing::instrument(skip_all, err, fields(name))]
    fn add_field(
        &mut self,
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<(), Error> {
        debug_assert_eq!(node.entry().tag(), gimli::DW_TAG_enumerator);

        let name = match node.entry().name(unit)? {
            Some(name) => name,
            None => format!("__unnamed_{:x}", self.fields.len()),
        };
        tracing::Span::current().record("name", &name);

        let value = match node
            .entry()
            .attr(gimli::DW_AT_const_value)?
            .as_ref()
            .map(Attribute::value)
        {
            Some(value) => {
                // TODO: assign correct type to variant.
                if let Some(value) = value.udata_value() {
                    Variant::U64(value)
                }
                else if let Some(value) = value.sdata_value() {
                    Variant::I64(value)
                }
                else {
                    tracing::warn!(?value, "enumerator has invalid value");
                    return Ok(());
                }
            }
            None => {
                tracing::warn!("enumerator doesn't have a value");
                return Ok(());
            }
        };

        self.fields.insert(name.into(), value);

        Ok(())
    }
}

impl<'data> DwarfType<'data> for Type<'data> {
    fn new(
        unit: &UnitRef<Reader<'data>>,
        node: EntriesTreeNode<Reader<'data>>,
    ) -> Result<Self, Error> {
        let type_ = match node.entry().type_(unit)? {
            Some(type_) => type_,
            None => {
                // If the type is not found, it's probably a void type.
                return Ok(Self::Base(BaseRef::Void));
            }
        };

        if let Some(bit_length) = node.entry().bit_size()? {
            let bit_position = node.entry().data_bit_offset()?.unwrap_or(0) % 8;

            return Ok(Self::Bitfield(BitfieldRef {
                bit_length,
                bit_position,
                subtype: Box::new(Self::from_type(unit, type_)?),
            }));
        }

        Self::from_type(unit, type_)
    }

    fn from_type(
        unit: &UnitRef<Reader<'data>>,
        mut type_: EntriesTree<Reader<'data>>,
    ) -> Result<Self, Error> {
        let node = type_.root()?;

        let result = match node.entry().tag() {
            gimli::DW_TAG_base_type => Self::Base(__type_from_base_type(unit, node)?),

            gimli::DW_TAG_enumeration_type => Self::Enum(EnumRef {
                name: type_name(unit, node.entry())?,
            }),

            gimli::DW_TAG_structure_type | gimli::DW_TAG_union_type => Self::Struct(StructRef {
                name: type_name(unit, node.entry())?,
            }),

            gimli::DW_TAG_array_type => Self::Array(__type_from_array_type(unit, type_)?),

            gimli::DW_TAG_pointer_type => Self::Pointer(PointerRef {
                subtype: Box::new(Self::new(unit, node)?),
            }),

            gimli::DW_TAG_subroutine_type => Self::Function,

            gimli::DW_TAG_typedef | gimli::DW_TAG_const_type | gimli::DW_TAG_volatile_type => {
                Self::new(unit, node)?
            }

            tag => {
                // dump_attrs(unit, node.entry())?;

                tracing::error!(?tag, "unexpected tag");
                Self::Base(BaseRef::Void)
            }
        };

        Ok(result)
    }
}

#[tracing::instrument(skip_all, err, fields(name))]
fn __type_from_base_type<'data>(
    unit: &UnitRef<Reader<'data>>,
    node: EntriesTreeNode<Reader<'data>>,
) -> Result<BaseRef, Error> {
    debug_assert_eq!(node.entry().tag(), gimli::DW_TAG_base_type);

    let name = type_name(unit, node.entry())?;
    tracing::Span::current().record("name", &*name);

    let byte_size = match node.entry().byte_size()? {
        Some(byte_size) => byte_size,
        None => {
            tracing::warn!("base type doesn't have a byte size");
            return Ok(BaseRef::Void);
        }
    };

    if byte_size == 0 {
        return Ok(BaseRef::Void);
    }

    let encoding = match node.entry().encoding()? {
        Some(encoding) => encoding,
        None => {
            tracing::warn!("base type doesn't have an encoding");
            return Ok(match byte_size {
                1 => BaseRef::U8,
                2 => BaseRef::U16,
                4 => BaseRef::U32,
                8 => BaseRef::U64,
                16 => BaseRef::U128,
                _ => {
                    tracing::error!(byte_size, "unsupported base type");
                    BaseRef::Void
                }
            });
        }
    };

    let result = match encoding {
        gimli::DW_ATE_boolean => match byte_size {
            1 => BaseRef::Bool,
            _ => {
                tracing::error!(byte_size, "unsupported boolean base type");
                BaseRef::Void
            }
        },
        gimli::DW_ATE_signed | gimli::DW_ATE_signed_char => match byte_size {
            1 => BaseRef::I8,
            2 => BaseRef::I16,
            4 => BaseRef::I32,
            8 => BaseRef::I64,
            16 => BaseRef::I128,
            _ => {
                tracing::error!(byte_size, "unsupported signed base type");
                BaseRef::Void
            }
        },
        gimli::DW_ATE_unsigned | gimli::DW_ATE_unsigned_char => match byte_size {
            1 => BaseRef::U8,
            2 => BaseRef::U16,
            4 => BaseRef::U32,
            8 => BaseRef::U64,
            16 => BaseRef::U128,
            _ => {
                tracing::error!(byte_size, "unsupported unsigned base type");
                BaseRef::Void
            }
        },
        gimli::DW_ATE_float => match byte_size {
            4 => BaseRef::F32,
            8 => BaseRef::F64,
            _ => {
                tracing::error!(byte_size, "unsupported float base type");
                BaseRef::Void
            }
        },
        _ => match byte_size {
            1 => BaseRef::U8,
            2 => BaseRef::U16,
            4 => BaseRef::U32,
            8 => BaseRef::U64,
            16 => BaseRef::U128,
            _ => {
                tracing::error!(?encoding, byte_size, "unsupported base type");
                BaseRef::Void
            }
        },
    };

    Ok(result)
}

fn __type_from_array_type<'data>(
    unit: &UnitRef<Reader<'data>>,
    mut type_: EntriesTree<Reader<'data>>,
) -> Result<ArrayRef<'data>, Error> {
    let node = type_.root()?;
    debug_assert_eq!(node.entry().tag(), gimli::DW_TAG_array_type);

    let mut dimensions = SmallVec::<[Option<u64>; 4]>::new();
    let mut children = node.children();

    // Parse array dimensions.
    while let Some(child) = children.next()? {
        if child.entry().tag() != gimli::DW_TAG_subrange_type {
            continue;
        }

        let count = match child.entry().count()? {
            Some(count) => Some(count),

            // Old binaries may have an upper bound instead.
            None => child
                .entry()
                .upper_bound()?
                .map(|upper_bound| upper_bound + 1),
        };

        dimensions.push(count);
    }

    let count = dimensions.first().copied().flatten().unwrap_or(0);

    // Parse the type again, since the node.children() iterator consumed the node.
    let node = type_.root()?;

    Ok(ArrayRef {
        subtype: Box::new(Type::new(unit, node)?),
        dims: dimensions.into_iter().map(|dim| dim.unwrap_or(0)).collect(),
        size: count,
    })
}

fn __dump_attrs<'data>(
    unit: &UnitRef<Reader<'data>>,
    entry: &DebuggingInformationEntry<Reader<'data>>,
) -> Result<(), Error> {
    let mut attrs = entry.attrs();
    while let Some(attr) = attrs.next()? {
        print!("   {}: {:?}", attr.name(), attr.value());
        if let Ok(s) = unit.attr_string(attr.value()) {
            print!(" '{}'", s.to_string_lossy()?);
        }
        println!();
    }

    Ok(())
}
