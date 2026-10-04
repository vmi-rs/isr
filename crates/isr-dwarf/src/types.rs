//! DWARF type extraction.

use std::collections::{HashMap, hash_map};

use gimli::{
    AttributeValue, DebuggingInformationEntry, DwTag, EntriesTreeNode, Error, UnitOffset, UnitRef,
};
use indexmap::{IndexMap, map::Entry};
use isr_core::schema::{
    Array, Base, Bitfield, Enum, EnumRef, Field, Pointer, Profile, Struct, StructKind, StructRef,
    Type, Variant,
};

use super::{
    _gimli::{DebuggingInformationEntryExt as _, Reader},
    units::DwarfUnits,
};

/// LLVM pointer authentication qualifier (`DW_TAG_LLVM_ptrauth_type`).
const DW_TAG_LLVM_PTRAUTH_TYPE: DwTag = DwTag(0x4300);

/// LLVM source annotation (`DW_TAG_LLVM_annotation`) attached to members and
/// types by `btf_decl_tag`.
const DW_TAG_LLVM_ANNOTATION: DwTag = DwTag(0x6000);

/// Populates a [`Profile`] with types extracted from DWARF units.
pub trait DwarfTypes<'data> {
    /// Adds every enum, struct, union and class definition found in `units`.
    ///
    /// Definitions nested inside other structs, unions and classes are added
    /// as well. Copies of the same definition emitted into several units are
    /// added once, and anonymous copies resolve to the name of the added copy.
    fn add_types(&mut self, units: &DwarfUnits<'data>) -> Result<(), Error>;
}

impl<'data> DwarfTypes<'data> for Profile {
    fn add_types(&mut self, units: &DwarfUnits<'data>) -> Result<(), Error> {
        let mut index = DeclIndex::default();
        for (unit_index, unit) in units.iter().enumerate() {
            tracing::debug!("indexing types: {}/{}", unit_index + 1, units.len());
            index.add_unit(unit)?;
        }

        let collector = TypeCollector {
            units,
            duplicates: index.duplicates,
        };

        for (unit_index, unit) in units.iter().enumerate() {
            tracing::debug!("collecting types: {}/{}", unit_index + 1, units.len());
            collector.add_unit(self, unit)?;
        }

        Ok(())
    }
}

/// Returns the struct kind for tags that define a struct-like type.
fn struct_kind(tag: DwTag) -> Option<StructKind> {
    match tag {
        gimli::DW_TAG_structure_type => Some(StructKind::Struct),
        gimli::DW_TAG_class_type => Some(StructKind::Class),
        gimli::DW_TAG_union_type => Some(StructKind::Union),
        _ => None,
    }
}

/// Returns `true` for tags of type definitions added to the profile.
fn is_definition_tag(tag: DwTag) -> bool {
    tag == gimli::DW_TAG_enumeration_type || struct_kind(tag).is_some()
}

/// Returns the offset of `entry` within its section.
///
/// The offset is unique across all units and names anonymous types.
fn section_offset(
    unit: UnitRef<'_, Reader<'_>>,
    entry: &DebuggingInformationEntry<Reader<'_>>,
) -> usize {
    entry.offset().to_unit_section_offset(&unit).0
}

/// Returns the integer type of an enum without `DW_AT_type`, derived from
/// its byte size and the sign of its enumerators.
fn enum_subtype(byte_size: Option<u64>, signed: bool) -> Base {
    match (byte_size, signed) {
        (Some(1), false) => Base::U8,
        (Some(2), false) => Base::U16,
        (Some(4), false) => Base::U32,
        (Some(8), false) => Base::U64,
        (Some(16), false) => Base::U128,
        (Some(1), true) => Base::I8,
        (Some(2), true) => Base::I16,
        (Some(4), true) => Base::I32,
        (Some(8), true) => Base::I64,
        (Some(16), true) => Base::I128,
        (byte_size, _) => {
            tracing::warn!(?byte_size, "unsupported enum byte size");
            Base::Void
        }
    }
}

/// Source identity of a type definition.
///
/// Compilers emit a copy of every used type into each unit. Copies of the
/// same definition share this key, which lets only the first copy be added.
#[derive(PartialEq, Eq, Hash)]
struct DeclKey {
    tag: DwTag,
    name: Option<String>,
    file: String,
    line: u64,
    column: Option<u64>,

    /// Member or enumerator names and offsets of an anonymous type.
    ///
    /// Without `DW_AT_decl_column`, anonymous types declared on the same line
    /// are only distinguishable by their contents. Named types leave this
    /// empty.
    layout: Vec<(Option<String>, u64)>,
}

/// First pass over all units that finds duplicate copies of definitions.
#[derive(Default)]
struct DeclIndex {
    /// Section offset of the first copy of each definition.
    first: HashMap<DeclKey, usize>,

    /// Section offset of every later copy, mapped to the offset of the first.
    duplicates: HashMap<usize, usize>,
}

impl DeclIndex {
    /// Indexes the definitions of a unit in the order used by
    /// [`TypeCollector::add_unit`].
    fn add_unit<'data>(&mut self, unit: UnitRef<'_, Reader<'data>>) -> Result<(), Error> {
        let mut tree = unit.entries_tree(None)?;
        let mut children = tree.root()?.children();
        while let Some(child) = children.next()? {
            self.add_definition(unit, child)?;
        }

        Ok(())
    }

    /// Indexes a definition and the definitions nested inside it.
    fn add_definition<'data>(
        &mut self,
        unit: UnitRef<'_, Reader<'data>>,
        node: EntriesTreeNode<'_, '_, Reader<'data>>,
    ) -> Result<(), Error> {
        let entry = node.entry();
        let tag = entry.tag();
        if !is_definition_tag(tag) || entry.declaration()?.unwrap_or(false) {
            return Ok(());
        }

        let offset = section_offset(unit, entry);
        let name = entry.name(&unit)?;
        let file = entry.decl_file(&unit)?;
        let line = entry.decl_line()?;
        let column = entry.decl_column()?;
        let anonymous = name.is_none();

        let mut layout = Vec::new();
        let mut children = node.children();
        while let Some(child) = children.next()? {
            let child_entry = child.entry();
            match child_entry.tag() {
                gimli::DW_TAG_member | gimli::DW_TAG_enumerator if anonymous => {
                    let location = match child_entry.data_member_location()? {
                        Some(location) => location,
                        None => child_entry.data_bit_offset()?.unwrap_or(0),
                    };
                    layout.push((child_entry.name(&unit)?, location));
                }
                child_tag if is_definition_tag(child_tag) => self.add_definition(unit, child)?,
                _ => (),
            }
        }

        let (file, line) = match (file, line) {
            (Some(file), Some(line)) => (file, line),
            _ => {
                tracing::trace!(?name, offset, "missing declaration information");
                return Ok(());
            }
        };

        let key = DeclKey {
            tag,
            name,
            file,
            line,
            column,
            layout,
        };

        match self.first.entry(key) {
            hash_map::Entry::Vacant(entry) => {
                entry.insert(offset);
            }
            hash_map::Entry::Occupied(entry) => {
                self.duplicates.insert(offset, *entry.get());
            }
        }

        Ok(())
    }
}

/// Second pass over all units that converts definitions into profile types.
struct TypeCollector<'u, 'data> {
    units: &'u DwarfUnits<'data>,

    /// Section offsets of skipped copies, mapped to the offset of the copy
    /// added to the profile.
    duplicates: HashMap<usize, usize>,
}

impl<'u, 'data> TypeCollector<'u, 'data> {
    /// Adds the definitions of a unit to `profile`.
    fn add_unit(
        &self,
        profile: &mut Profile,
        unit: UnitRef<'u, Reader<'data>>,
    ) -> Result<(), Error> {
        let mut tree = unit.entries_tree(None)?;
        let mut children = tree.root()?.children();
        while let Some(child) = children.next()? {
            self.add_definition(profile, unit, child)?;
        }

        Ok(())
    }

    /// Adds a definition and the definitions nested inside it, unless it is a
    /// declaration or a copy of a definition that is already added.
    fn add_definition(
        &self,
        profile: &mut Profile,
        unit: UnitRef<'u, Reader<'data>>,
        node: EntriesTreeNode<'_, '_, Reader<'data>>,
    ) -> Result<(), Error> {
        let entry = node.entry();
        if !is_definition_tag(entry.tag()) || entry.declaration()?.unwrap_or(false) {
            return Ok(());
        }

        if self.duplicates.contains_key(&section_offset(unit, entry)) {
            return Ok(());
        }

        match struct_kind(entry.tag()) {
            Some(kind) => self.add_struct(profile, unit, node, kind),
            None => self.add_enum(profile, unit, node),
        }
    }

    /// Returns the profile name of a type definition.
    ///
    /// Anonymous types are named after the section offset of the copy added
    /// to the profile.
    fn type_name(
        &self,
        unit: UnitRef<'_, Reader<'data>>,
        entry: &DebuggingInformationEntry<Reader<'data>>,
    ) -> Result<String, Error> {
        match entry.name(&unit)? {
            Some(name) => Ok(name),
            None => {
                let offset = section_offset(unit, entry);
                let offset = match self.duplicates.get(&offset) {
                    Some(&canonical) => canonical,
                    None => offset,
                };

                Ok(format!("__unnamed_{offset:x}"))
            }
        }
    }

    /// Adds an enumeration DIE and its variants.
    #[tracing::instrument(skip_all, err, fields(name))]
    fn add_enum(
        &self,
        profile: &mut Profile,
        unit: UnitRef<'u, Reader<'data>>,
        node: EntriesTreeNode<'_, '_, Reader<'data>>,
    ) -> Result<(), Error> {
        let entry = node.entry();
        let name = self.type_name(unit, entry)?;
        tracing::Span::current().record("name", &*name);

        let subtype = match entry.attr_value(gimli::DW_AT_type) {
            Some(_) => Some(self.attr_type(unit, entry)?),
            None => None,
        };
        let byte_size = entry.byte_size()?;

        let mut fields = Default::default();
        let mut signed = false;
        let mut children = node.children();
        while let Some(child) = children.next()? {
            if child.entry().tag() != gimli::DW_TAG_enumerator {
                tracing::warn!(
                    tag = ?child.entry().tag(),
                    "unexpected tag (expected DW_TAG_enumerator)"
                );

                continue;
            }

            signed |= add_enumerator(&mut fields, unit, child.entry())?;
        }

        let new_enum = Enum {
            subtype: match subtype {
                Some(subtype) => subtype,
                None => Type::Base(enum_subtype(byte_size, signed)),
            },
            fields,
        };

        let new_enum_fields = new_enum.fields.len();

        match profile.enums.entry(name.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(new_enum);
            }
            Entry::Occupied(mut entry) => {
                let previous_enum = entry.get_mut();
                let previous_enum_fields = previous_enum.fields.len();

                if new_enum_fields > previous_enum_fields {
                    tracing::warn!(
                        %name,
                        new_enum_fields,
                        previous_enum_fields,
                        "duplicate enum name; overwriting"
                    );

                    *previous_enum = new_enum;
                }
            }
        }

        Ok(())
    }

    /// Adds a struct, union or class DIE, its fields, and the definitions
    /// nested inside it.
    #[tracing::instrument(skip_all, err, fields(name))]
    fn add_struct(
        &self,
        profile: &mut Profile,
        unit: UnitRef<'u, Reader<'data>>,
        node: EntriesTreeNode<'_, '_, Reader<'data>>,
        kind: StructKind,
    ) -> Result<(), Error> {
        let name = self.type_name(unit, node.entry())?;
        tracing::Span::current().record("name", &*name);

        let mut new_udt = Struct {
            kind,
            size: node.entry().byte_size()?.unwrap_or(0),
            fields: Default::default(),
        };

        let mut children = node.children();
        while let Some(child) = children.next()? {
            match child.entry().tag() {
                gimli::DW_TAG_member => self.add_field(&mut new_udt, unit, child.entry())?,

                tag if is_definition_tag(tag) => self.add_definition(profile, unit, child)?,

                // Typedefs are resolved where they are used. Methods, template
                // parameters and annotations do not affect the layout. Base
                // classes and static members are not represented as fields.
                gimli::DW_TAG_typedef
                | gimli::DW_TAG_subprogram
                | gimli::DW_TAG_template_type_parameter
                | gimli::DW_TAG_template_value_parameter
                | gimli::DW_TAG_GNU_template_template_param
                | gimli::DW_TAG_GNU_template_parameter_pack
                | gimli::DW_TAG_inheritance
                | gimli::DW_TAG_variable
                | DW_TAG_LLVM_ANNOTATION => (),

                tag => {
                    tracing::warn!(?tag, "unexpected tag (expected DW_TAG_member)");
                }
            }
        }

        let new_udt_fields = new_udt.fields.len();

        match profile.structs.entry(name.clone()) {
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

    /// Adds a `DW_TAG_member` to `udt`.
    #[tracing::instrument(skip_all, err, fields(name))]
    fn add_field(
        &self,
        udt: &mut Struct,
        unit: UnitRef<'u, Reader<'data>>,
        entry: &DebuggingInformationEntry<Reader<'data>>,
    ) -> Result<(), Error> {
        // Static data members are declarations without a location.
        if entry.declaration()?.unwrap_or(false) {
            return Ok(());
        }

        let name = match entry.name(&unit)? {
            Some(name) => name,
            None => format!("__unnamed_field_{:x}", udt.fields.len()),
        };
        tracing::Span::current().record("name", &name);

        let offset = match entry.data_member_location()? {
            Some(offset) => offset,
            None => match entry.data_bit_offset()? {
                Some(bit_offset) => bit_offset / 8,
                // Assume zero offset if no offset is found.
                None => 0,
            },
        };

        let ty = match entry.bit_size()? {
            Some(bit_length) => Type::Bitfield(Bitfield {
                bit_length,
                bit_position: entry.data_bit_offset()?.unwrap_or(0) % 8,
                subtype: Box::new(self.attr_type(unit, entry)?),
            }),
            None => self.attr_type(unit, entry)?,
        };

        udt.fields.insert(name, Field { offset, ty });

        Ok(())
    }

    /// Returns the type referenced by the `DW_AT_type` attribute of `entry`.
    ///
    /// A missing attribute denotes `void`.
    fn attr_type(
        &self,
        unit: UnitRef<'u, Reader<'data>>,
        entry: &DebuggingInformationEntry<Reader<'data>>,
    ) -> Result<Type, Error> {
        let value = match entry.attr_value(gimli::DW_AT_type) {
            Some(value) => value,
            None => return Ok(Type::Base(Base::Void)),
        };

        match self.units.resolve(unit, value.clone()) {
            Some((unit, offset)) => self.type_at(unit, offset),
            None => {
                tracing::warn!(?value, "unsupported type reference");
                Ok(Type::Base(Base::Void))
            }
        }
    }

    /// Returns the type defined by the DIE at `offset` in `unit`.
    fn type_at(&self, unit: UnitRef<'u, Reader<'data>>, offset: UnitOffset) -> Result<Type, Error> {
        let mut tree = unit.entries_tree(Some(offset))?;
        let node = tree.root()?;
        let entry = node.entry();

        let result = match entry.tag() {
            gimli::DW_TAG_base_type => Type::Base(base_type(&unit, entry)?),

            gimli::DW_TAG_enumeration_type => Type::Enum(EnumRef {
                name: self.type_name(unit, entry)?,
            }),

            gimli::DW_TAG_structure_type | gimli::DW_TAG_class_type | gimli::DW_TAG_union_type => {
                Type::Struct(StructRef {
                    name: self.type_name(unit, entry)?,
                })
            }

            gimli::DW_TAG_array_type => Type::Array(self.array_type(unit, node)?),

            gimli::DW_TAG_pointer_type
            | gimli::DW_TAG_reference_type
            | gimli::DW_TAG_rvalue_reference_type => {
                // Clang omits the byte size of pointers.
                let size = match entry.byte_size()? {
                    Some(size) => size,
                    None => u64::from(unit.encoding().address_size),
                };

                Type::Pointer(Pointer {
                    subtype: Box::new(self.attr_type(unit, entry)?),
                    size,
                })
            }

            gimli::DW_TAG_subroutine_type => Type::Function,

            // Qualifiers and aliases do not change the layout.
            gimli::DW_TAG_typedef
            | gimli::DW_TAG_const_type
            | gimli::DW_TAG_volatile_type
            | gimli::DW_TAG_restrict_type
            | gimli::DW_TAG_atomic_type
            | gimli::DW_TAG_immutable_type
            | DW_TAG_LLVM_PTRAUTH_TYPE => self.attr_type(unit, entry)?,

            gimli::DW_TAG_unspecified_type => Type::Base(Base::Void),

            tag => {
                tracing::error!(?tag, "unexpected tag");
                Type::Base(Base::Void)
            }
        };

        Ok(result)
    }

    /// Returns the array type described by `node` and its subranges.
    fn array_type(
        &self,
        unit: UnitRef<'u, Reader<'data>>,
        node: EntriesTreeNode<'_, '_, Reader<'data>>,
    ) -> Result<Array, Error> {
        debug_assert_eq!(node.entry().tag(), gimli::DW_TAG_array_type);

        let subtype = self.attr_type(unit, node.entry())?;

        let mut dims = Vec::new();
        let mut children = node.children();
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

            dims.push(count.unwrap_or(0));
        }

        Ok(Array {
            subtype: Box::new(subtype),
            dims,
        })
    }
}

/// Inserts an enumerator into `fields` and returns `true` if its value is
/// explicitly encoded as negative.
#[tracing::instrument(skip_all, err, fields(name))]
fn add_enumerator<'data>(
    fields: &mut IndexMap<String, Variant>,
    unit: UnitRef<'_, Reader<'data>>,
    entry: &DebuggingInformationEntry<Reader<'data>>,
) -> Result<bool, Error> {
    debug_assert_eq!(entry.tag(), gimli::DW_TAG_enumerator);

    let name = match entry.name(&unit)? {
        Some(name) => name,
        None => format!("__unnamed_{:x}", fields.len()),
    };
    tracing::Span::current().record("name", &name);

    let value = match entry.attr_value(gimli::DW_AT_const_value) {
        Some(value) => value,
        None => {
            tracing::warn!("enumerator doesn't have a value");
            return Ok(false);
        }
    };

    let negative = matches!(value, AttributeValue::Sdata(value) if value < 0);

    // TODO: assign correct type to variant.
    let variant = if let Some(value) = value.udata_value() {
        Variant::U64(value)
    }
    else if let Some(value) = value.sdata_value() {
        Variant::I64(value)
    }
    else {
        tracing::warn!(?value, "enumerator has invalid value");
        return Ok(negative);
    };

    fields.insert(name, variant);

    Ok(negative)
}

/// Converts a `DW_TAG_base_type` DIE into a base type.
#[tracing::instrument(skip_all, err, fields(name))]
fn base_type<'data>(
    unit: &UnitRef<'_, Reader<'data>>,
    entry: &DebuggingInformationEntry<Reader<'data>>,
) -> Result<Base, Error> {
    debug_assert_eq!(entry.tag(), gimli::DW_TAG_base_type);

    if let Some(name) = entry.name(unit)? {
        tracing::Span::current().record("name", &*name);
    }

    let byte_size = match entry.byte_size()? {
        Some(byte_size) => byte_size,
        None => {
            tracing::warn!("base type doesn't have a byte size");
            return Ok(Base::Void);
        }
    };

    if byte_size == 0 {
        return Ok(Base::Void);
    }

    let encoding = match entry.encoding()? {
        Some(encoding) => encoding,
        None => {
            tracing::warn!("base type doesn't have an encoding");
            return Ok(match byte_size {
                1 => Base::U8,
                2 => Base::U16,
                4 => Base::U32,
                8 => Base::U64,
                16 => Base::U128,
                _ => {
                    tracing::error!(byte_size, "unsupported base type");
                    Base::Void
                }
            });
        }
    };

    let result = match encoding {
        gimli::DW_ATE_boolean => match byte_size {
            1 => Base::Bool,
            _ => {
                tracing::error!(byte_size, "unsupported boolean base type");
                Base::Void
            }
        },
        gimli::DW_ATE_signed | gimli::DW_ATE_signed_char => match byte_size {
            1 => Base::I8,
            2 => Base::I16,
            4 => Base::I32,
            8 => Base::I64,
            16 => Base::I128,
            _ => {
                tracing::error!(byte_size, "unsupported signed base type");
                Base::Void
            }
        },
        gimli::DW_ATE_unsigned | gimli::DW_ATE_unsigned_char => match byte_size {
            1 => Base::U8,
            2 => Base::U16,
            4 => Base::U32,
            8 => Base::U64,
            16 => Base::U128,
            _ => {
                tracing::error!(byte_size, "unsupported unsigned base type");
                Base::Void
            }
        },
        gimli::DW_ATE_float => match byte_size {
            4 => Base::F32,
            8 => Base::F64,
            _ => {
                tracing::error!(byte_size, "unsupported float base type");
                Base::Void
            }
        },
        _ => match byte_size {
            1 => Base::U8,
            2 => Base::U16,
            4 => Base::U32,
            8 => Base::U64,
            16 => Base::U128,
            _ => {
                tracing::error!(?encoding, byte_size, "unsupported base type");
                Base::Void
            }
        },
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    use isr_core::schema::Base;

    use super::enum_subtype;

    #[test]
    fn enum_subtype_follows_byte_size_and_sign() {
        assert!(matches!(enum_subtype(Some(1), false), Base::U8));
        assert!(matches!(enum_subtype(Some(2), false), Base::U16));
        assert!(matches!(enum_subtype(Some(4), false), Base::U32));
        assert!(matches!(enum_subtype(Some(8), false), Base::U64));
        assert!(matches!(enum_subtype(Some(1), true), Base::I8));
        assert!(matches!(enum_subtype(Some(4), true), Base::I32));
        assert!(matches!(enum_subtype(Some(8), true), Base::I64));
    }

    #[test]
    fn enum_subtype_without_valid_size_is_void() {
        assert!(matches!(enum_subtype(None, false), Base::Void));
        assert!(matches!(enum_subtype(Some(3), true), Base::Void));
    }
}
