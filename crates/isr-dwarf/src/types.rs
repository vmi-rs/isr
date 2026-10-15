//! DWARF type extraction.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, hash_map},
};

use gimli::{
    AttributeValue, DebuggingInformationEntry, DwTag, EntriesTreeNode, Error, UnitOffset, UnitRef,
};
use indexmap::{IndexMap, IndexSet, map::Entry};
use isr_core::schema::{
    ANONYMOUS_FIELD_PREFIX, Array, Base, Bitfield, Enum, EnumRef, Field, Pointer, Profile, Struct,
    StructKind, StructRef, Type, Variant,
};

use super::{
    _gimli::{DebuggingInformationEntryExt as _, Reader},
    units::{DwarfUnits, Reference},
};

/// LLVM pointer authentication qualifier (`DW_TAG_LLVM_ptrauth_type`).
const DW_TAG_LLVM_PTRAUTH_TYPE: DwTag = DwTag(0x4300);

/// LLVM source annotation (`DW_TAG_LLVM_annotation`) attached to members and
/// types by `btf_decl_tag`.
const DW_TAG_LLVM_ANNOTATION: DwTag = DwTag(0x6000);

/// Populates a [`Profile`] with types and function prototypes extracted from
/// DWARF units.
pub trait DwarfTypes<'data> {
    /// Adds every enum, struct, union and class definition and every
    /// external function prototype found in `units`.
    ///
    /// Definitions nested inside other structs, unions and classes are added
    /// as well. Copies of the same definition emitted into several units are
    /// added once, and anonymous copies resolve to the name of the added copy.
    ///
    /// In C++ units, types declared inside a class or namespace are named
    /// with their qualified scope, for example `Outer::Nested`. Types of
    /// other units keep their unqualified name.
    ///
    /// An anonymous definition named by a file or namespace scope typedef,
    /// as in `typedef struct { ... } name_t`, is added under the typedef
    /// name, which every reference to it uses as well. It keeps an
    /// `__unnamed_<offset>` name if a named definition of the same kind uses
    /// the typedef name or the name is given to anonymous definitions of
    /// different layouts.
    ///
    /// Function prototypes come from subprograms in unit or namespace scope
    /// that are visible outside their unit (`DW_AT_external`), indexed by
    /// their linkage name if they have one and by their name otherwise.
    /// Definitions are preferred over declarations, and among those the
    /// copy with the most parameters is kept.
    fn add_types(&mut self, units: &DwarfUnits<'data>) -> Result<(), Error>;
}

impl<'data> DwarfTypes<'data> for Profile {
    fn add_types(&mut self, units: &DwarfUnits<'data>) -> Result<(), Error> {
        let mut index = DeclIndex::default();
        for unit_index in 0..units.len() {
            tracing::debug!("indexing types: {}/{}", unit_index + 1, units.len());
            let unit = units.unit(unit_index)?;
            index.add_unit(units.unit_ref(&unit))?;
        }

        let typedef_names = index.typedef_names();
        let collector = TypeCollector {
            units,
            duplicates: index.duplicates,
            qualified_names: index.qualified_names,
            typedef_names,
            defined_functions: RefCell::default(),
        };

        for unit_index in 0..units.len() {
            tracing::debug!("collecting types: {}/{}", unit_index + 1, units.len());
            let unit = units.unit(unit_index)?;
            collector.add_unit(self, units.unit_ref(&unit))?;
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

/// Returns `true` if the unit root `entry` declares a C++ dialect.
///
/// Nested types of C++ units live in the scope of their parent, while C
/// nested struct tags live in file scope.
fn is_cplusplus(entry: &DebuggingInformationEntry<Reader<'_>>) -> bool {
    matches!(
        entry.attr_value(gimli::DW_AT_language),
        Some(AttributeValue::Language(
            gimli::DW_LANG_C_plus_plus
                | gimli::DW_LANG_C_plus_plus_03
                | gimli::DW_LANG_C_plus_plus_11
                | gimli::DW_LANG_C_plus_plus_14
                | gimli::DW_LANG_C_plus_plus_17
                | gimli::DW_LANG_C_plus_plus_20
                | gimli::DW_LANG_ObjC_plus_plus
        ))
    )
}

/// Returns the scope name of a `DW_TAG_namespace` entry.
///
/// Anonymous namespaces are named like Clang and LLDB print them.
fn namespace_name<'data>(
    unit: UnitRef<'_, Reader<'data>>,
    entry: &DebuggingInformationEntry<Reader<'data>>,
) -> Result<String, Error> {
    match entry.name(&unit)? {
        Some(name) => Ok(name),
        None => Ok(String::from("(anonymous namespace)")),
    }
}

/// Returns `name` qualified with the C++ `scope` it is declared in.
fn qualify(scope: &str, name: &str) -> String {
    if scope.is_empty() {
        name.to_owned()
    }
    else {
        format!("{scope}::{name}")
    }
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
    /// DWARF tag of the definition.
    tag: DwTag,

    /// Qualified name of the enclosing C++ scope.
    ///
    /// Empty for types declared at file scope and for every type of a non-C++
    /// unit. Keeps same-named nested types of different parents apart.
    scope: String,

    /// Unqualified name, `None` for anonymous types.
    name: Option<String>,

    /// Path of the declaring source file.
    file: String,

    /// Line of the declaration in `file`.
    line: u64,

    /// Column of the declaration, if the compiler emits it.
    column: Option<u64>,

    /// Member or enumerator names and offsets of an anonymous type.
    ///
    /// Without `DW_AT_decl_column`, anonymous types declared on the same line
    /// are only distinguishable by their contents. Named types leave this
    /// empty.
    layout: Vec<(Option<String>, u64)>,
}

/// First pass over all units that finds duplicate copies of definitions and
/// the typedefs that name anonymous definitions.
#[derive(Default)]
struct DeclIndex {
    /// Section offset of the first copy of each definition.
    first: HashMap<DeclKey, usize>,

    /// Section offset of every later copy, mapped to the offset of the first.
    duplicates: HashMap<usize, usize>,

    /// Qualified names of named types declared inside a C++ class or
    /// namespace, keyed by section offset.
    ///
    /// Covers declarations as well as definitions, so references to either
    /// resolve to the same name.
    qualified_names: HashMap<usize, String>,

    /// Tags of the first copies of anonymous definitions, indexed by section
    /// offset.
    anonymous: HashMap<usize, DwTag>,

    /// Profile names of named struct, union and class definitions.
    struct_names: HashSet<String>,

    /// Profile names of named enum definitions.
    enum_names: HashSet<String>,

    /// Section offset of the named type and qualified name of each typedef
    /// of the unit being indexed.
    unit_typedefs: Vec<(usize, String)>,

    /// Typedefs naming a type in a unit that is not indexed yet, as section
    /// offset of the type and qualified typedef name.
    forward_typedefs: Vec<(usize, String)>,

    /// Distinct pairs of the section offset of the first copy of an
    /// anonymous definition and the qualified name of a typedef naming it,
    /// in declaration order.
    typedefs: IndexSet<(usize, String)>,
}

impl DeclIndex {
    /// Indexes the definitions of a unit in the order used by
    /// [`TypeCollector::add_unit`].
    fn add_unit<'data>(&mut self, unit: UnitRef<'_, Reader<'data>>) -> Result<(), Error> {
        let mut tree = unit.entries_tree(None)?;
        let root = tree.root()?;
        let cplusplus = is_cplusplus(root.entry());
        self.add_scope(unit, root, "", cplusplus)?;

        // Every definition of this and the earlier units is indexed now.
        let unit_end = unit.header.offset().0 + unit.header.length_including_self();
        let typedefs = std::mem::take(&mut self.unit_typedefs);
        for (target, name) in typedefs {
            if target >= unit_end {
                self.forward_typedefs.push((target, name));
            }
            else {
                self.add_typedef_target(target, name);
            }
        }

        Ok(())
    }

    /// Indexes the definitions and typedefs declared directly in the unit or
    /// namespace `node`, whose qualified name is `scope`.
    fn add_scope<'data>(
        &mut self,
        unit: UnitRef<'_, Reader<'data>>,
        node: EntriesTreeNode<'_, '_, Reader<'data>>,
        scope: &str,
        cplusplus: bool,
    ) -> Result<(), Error> {
        let mut children = node.children();
        while let Some(child) = children.next()? {
            let tag = child.entry().tag();
            if is_definition_tag(tag) {
                self.add_definition(unit, child, scope, cplusplus)?;
            }
            else if tag == gimli::DW_TAG_typedef {
                self.add_typedef(unit, child.entry(), scope)?;
            }
            else if tag == gimli::DW_TAG_namespace && cplusplus {
                let name = namespace_name(unit, child.entry())?;
                self.add_scope(unit, child, &qualify(scope, &name), cplusplus)?;
            }
        }

        Ok(())
    }

    /// Indexes a definition declared in `scope` and the definitions nested
    /// inside it.
    ///
    /// Nested definitions are indexed even when `node` is a declaration or a
    /// duplicate, because compilers emit C++ nested types lazily and only
    /// some copies of the enclosing type carry them.
    fn add_definition<'data>(
        &mut self,
        unit: UnitRef<'_, Reader<'data>>,
        node: EntriesTreeNode<'_, '_, Reader<'data>>,
        scope: &str,
        cplusplus: bool,
    ) -> Result<(), Error> {
        let entry = node.entry();
        let tag = entry.tag();
        let offset = section_offset(unit, entry);
        let name = entry.name(&unit)?;
        let declaration = entry.declaration()?.unwrap_or(false);
        let anonymous = name.is_none();
        let file = if declaration {
            None
        }
        else {
            entry.decl_file(&unit)?
        };
        let line = entry.decl_line()?;
        let column = entry.decl_column()?;

        // Anonymous types do not open a scope of their own.
        let nested_scope = match &name {
            Some(name) if cplusplus => {
                let qualified_name = qualify(scope, name);
                if !scope.is_empty() {
                    self.qualified_names.insert(offset, qualified_name.clone());
                }

                qualified_name
            }
            _ => scope.to_owned(),
        };

        let mut layout = Vec::new();
        let mut children = node.children();
        while let Some(child) = children.next()? {
            let child_entry = child.entry();
            match child_entry.tag() {
                gimli::DW_TAG_member | gimli::DW_TAG_enumerator if anonymous && !declaration => {
                    let location = match child_entry.data_member_location()? {
                        Some(location) => location,
                        None => child_entry.data_bit_offset()?.unwrap_or(0),
                    };
                    layout.push((child_entry.name(&unit)?, location));
                }
                child_tag if is_definition_tag(child_tag) => {
                    self.add_definition(unit, child, &nested_scope, cplusplus)?;
                }
                _ => (),
            }
        }

        if declaration {
            return Ok(());
        }

        if let Some(name) = &name {
            let names = match tag {
                gimli::DW_TAG_enumeration_type => &mut self.enum_names,
                _ => &mut self.struct_names,
            };

            let profile_name = if cplusplus {
                qualify(scope, name)
            }
            else {
                name.clone()
            };

            if !names.contains(&profile_name) {
                names.insert(profile_name);
            }
        }

        let (file, line) = match (file, line) {
            (Some(file), Some(line)) => (file, line),
            _ => {
                tracing::trace!(?name, offset, "missing declaration information");
                if anonymous {
                    self.anonymous.insert(offset, tag);
                }

                return Ok(());
            }
        };

        let key = DeclKey {
            tag,
            scope: scope.to_owned(),
            name,
            file,
            line,
            column,
            layout,
        };

        match self.first.entry(key) {
            hash_map::Entry::Vacant(entry) => {
                entry.insert(offset);
                if anonymous {
                    self.anonymous.insert(offset, tag);
                }
            }
            hash_map::Entry::Occupied(entry) => {
                self.duplicates.insert(offset, *entry.get());
            }
        }

        Ok(())
    }

    /// Queues a typedef declared in `scope` for naming the type it refers
    /// to, once the unit is indexed.
    fn add_typedef<'data>(
        &mut self,
        unit: UnitRef<'_, Reader<'data>>,
        entry: &DebuggingInformationEntry<Reader<'data>>,
        scope: &str,
    ) -> Result<(), Error> {
        let name = match entry.name(&unit)? {
            Some(name) => name,
            None => return Ok(()),
        };

        let target = match entry.attr_value(gimli::DW_AT_type) {
            Some(AttributeValue::UnitRef(offset)) => offset.to_unit_section_offset(&unit).0,
            Some(AttributeValue::DebugInfoRef(offset)) => offset.0,
            _ => return Ok(()),
        };

        self.unit_typedefs.push((target, qualify(scope, &name)));
        Ok(())
    }

    /// Records the typedef `name` of the type at section offset `target` if
    /// that type is an anonymous definition.
    ///
    /// `target` must belong to an indexed unit.
    fn add_typedef_target(&mut self, target: usize, name: String) {
        let canonical = match self.duplicates.get(&target) {
            Some(&canonical) => canonical,
            None => target,
        };

        if self.anonymous.contains_key(&canonical) {
            self.typedefs.insert((canonical, name));
        }
    }

    /// Returns the typedef names of anonymous definitions, indexed by the
    /// section offset of the copy added to the profile.
    ///
    /// C code commonly declares a type as `typedef struct { ... } name_t`,
    /// whose definition has no name of its own. Such a definition takes the
    /// name of the typedef, unless
    ///
    /// - a named definition of the same kind already uses the name,
    /// - the typedef name is given to anonymous definitions of different
    ///   layouts, for example in different configurations of a header.
    ///
    /// In both cases the definitions keep their `__unnamed_*` names. Copies
    /// of the same layout that are declared in different files, such as a
    /// header and its installed copy, are merged into one definition. A
    /// definition named by several typedefs takes the first name.
    fn typedef_names(&mut self) -> HashMap<usize, String> {
        let forward_typedefs = std::mem::take(&mut self.forward_typedefs);
        for (target, name) in forward_typedefs {
            self.add_typedef_target(target, name);
        }

        // First copies of the anonymous definitions each typedef name names,
        // indexed by whether they are enums, because enums and structs are
        // kept in different maps of the profile.
        let mut candidates = IndexMap::<(bool, String), Vec<usize>>::new();
        for (offset, name) in std::mem::take(&mut self.typedefs) {
            let is_enum = self.anonymous[&offset] == gimli::DW_TAG_enumeration_type;
            candidates.entry((is_enum, name)).or_default().push(offset);
        }

        let ambiguous = candidates
            .values()
            .filter(|offsets| offsets.len() > 1)
            .flatten()
            .copied()
            .collect::<HashSet<_>>();
        let keys = self
            .first
            .iter()
            .filter(|(_, offset)| ambiguous.contains(offset))
            .map(|(key, &offset)| (offset, key))
            .collect::<HashMap<_, _>>();

        let mut names = HashMap::new();
        let mut merged = HashMap::new();
        for ((is_enum, name), offsets) in candidates {
            let named = if is_enum {
                &self.enum_names
            }
            else {
                &self.struct_names
            };

            if named.contains(&name) {
                tracing::warn!(%name, "typedef name of an anonymous type is used by a named type");
                continue;
            }

            let canonical = offsets[0];
            let same_layout =
                offsets[1..]
                    .iter()
                    .all(|offset| match (keys.get(&canonical), keys.get(offset)) {
                        (Some(first), Some(other)) => {
                            first.tag == other.tag && first.layout == other.layout
                        }
                        _ => false,
                    });

            if !same_layout {
                tracing::warn!(
                    %name,
                    copies = offsets.len(),
                    "typedef name is given to different anonymous types"
                );
                continue;
            }

            if names.contains_key(&canonical) {
                continue;
            }

            for &offset in &offsets[1..] {
                merged.insert(offset, canonical);
            }

            names.insert(canonical, name);
        }

        for canonical in self.duplicates.values_mut() {
            if let Some(&merged_canonical) = merged.get(canonical) {
                *canonical = merged_canonical;
            }
        }
        self.duplicates.extend(merged);

        names
    }
}

/// Second pass over all units that converts definitions into profile types
/// and subprograms into function prototypes.
pub(crate) struct TypeCollector<'u, 'data> {
    /// Units resolving cross-unit type references.
    pub(crate) units: &'u DwarfUnits<'data>,

    /// Section offsets of skipped copies, mapped to the offset of the copy
    /// added to the profile.
    duplicates: HashMap<usize, usize>,

    /// Qualified names of C++ types declared inside a class or namespace,
    /// keyed by section offset.
    qualified_names: HashMap<usize, String>,

    /// Typedef names of anonymous definitions, indexed by the section offset
    /// of the copy added to the profile.
    typedef_names: HashMap<usize, String>,

    /// Names of the functions whose prototype in the profile comes from a
    /// definition rather than a declaration.
    pub(crate) defined_functions: RefCell<HashSet<String>>,
}

impl<'u, 'data> TypeCollector<'u, 'data> {
    /// Adds the definitions of a unit to `profile`.
    fn add_unit(
        &self,
        profile: &mut Profile,
        unit: UnitRef<'_, Reader<'data>>,
    ) -> Result<(), Error> {
        let mut tree = unit.entries_tree(None)?;
        let root = tree.root()?;
        let cplusplus = is_cplusplus(root.entry());
        self.add_scope(profile, unit, root, cplusplus)
    }

    /// Adds the definitions and function prototypes declared directly in
    /// the unit or namespace `node`.
    fn add_scope(
        &self,
        profile: &mut Profile,
        unit: UnitRef<'_, Reader<'data>>,
        node: EntriesTreeNode<'_, '_, Reader<'data>>,
        cplusplus: bool,
    ) -> Result<(), Error> {
        let mut children = node.children();
        while let Some(child) = children.next()? {
            let tag = child.entry().tag();
            if is_definition_tag(tag) {
                self.add_definition(profile, unit, child)?;
            }
            else if tag == gimli::DW_TAG_subprogram {
                self.add_function(profile, unit, child)?;
            }
            else if tag == gimli::DW_TAG_namespace && cplusplus {
                self.add_scope(profile, unit, child, cplusplus)?;
            }
        }

        Ok(())
    }

    /// Adds a definition and the definitions nested inside it.
    ///
    /// A declaration or a copy of a definition that is already added is not
    /// added itself, but the definitions nested inside it still are, because
    /// compilers emit C++ nested types lazily and only some copies of the
    /// enclosing type carry them.
    fn add_definition(
        &self,
        profile: &mut Profile,
        unit: UnitRef<'_, Reader<'data>>,
        node: EntriesTreeNode<'_, '_, Reader<'data>>,
    ) -> Result<(), Error> {
        let entry = node.entry();
        if entry.declaration()?.unwrap_or(false)
            || self.duplicates.contains_key(&section_offset(unit, entry))
        {
            return self.add_nested_definitions(profile, unit, node);
        }

        match struct_kind(entry.tag()) {
            Some(kind) => self.add_struct(profile, unit, node, kind),
            None => self.add_enum(profile, unit, node),
        }
    }

    /// Adds the definitions nested inside `node` without adding `node`.
    fn add_nested_definitions(
        &self,
        profile: &mut Profile,
        unit: UnitRef<'_, Reader<'data>>,
        node: EntriesTreeNode<'_, '_, Reader<'data>>,
    ) -> Result<(), Error> {
        let mut children = node.children();
        while let Some(child) = children.next()? {
            if is_definition_tag(child.entry().tag()) {
                self.add_definition(profile, unit, child)?;
            }
        }

        Ok(())
    }

    /// Returns the profile name of a type definition or declaration.
    ///
    /// C++ types declared inside a class or namespace are named with their
    /// qualified scope. Anonymous types take the name of the typedef naming
    /// them, see [`DeclIndex::typedef_names`], or are named after the
    /// section offset of the copy added to the profile.
    fn type_name(
        &self,
        unit: UnitRef<'_, Reader<'data>>,
        entry: &DebuggingInformationEntry<Reader<'data>>,
    ) -> Result<String, Error> {
        let offset = section_offset(unit, entry);
        if let Some(qualified_name) = self.qualified_names.get(&offset) {
            return Ok(qualified_name.clone());
        }

        match entry.name(&unit)? {
            Some(name) => Ok(name),
            None => {
                let offset = match self.duplicates.get(&offset) {
                    Some(&canonical) => canonical,
                    None => offset,
                };

                match self.typedef_names.get(&offset) {
                    Some(name) => Ok(name.clone()),
                    None => Ok(format!("__unnamed_{offset:x}")),
                }
            }
        }
    }

    /// Adds an enumeration DIE and its variants.
    #[tracing::instrument(skip_all, err, fields(name))]
    fn add_enum(
        &self,
        profile: &mut Profile,
        unit: UnitRef<'_, Reader<'data>>,
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
        unit: UnitRef<'_, Reader<'data>>,
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
        unit: UnitRef<'_, Reader<'data>>,
        entry: &DebuggingInformationEntry<Reader<'data>>,
    ) -> Result<(), Error> {
        // Static data members are declarations without a location.
        if entry.declaration()?.unwrap_or(false) {
            return Ok(());
        }

        let name = match entry.name(&unit)? {
            Some(name) => name,
            None => format!("{ANONYMOUS_FIELD_PREFIX}{:x}", udt.fields.len()),
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
    pub(crate) fn attr_type(
        &self,
        unit: UnitRef<'_, Reader<'data>>,
        entry: &DebuggingInformationEntry<Reader<'data>>,
    ) -> Result<Type, Error> {
        let value = match entry.attr_value(gimli::DW_AT_type) {
            Some(value) => value,
            None => return Ok(Type::Base(Base::Void)),
        };

        match self.units.resolve(unit, value.clone())? {
            Some(Reference::Local(offset)) => self.type_at(unit, offset),
            Some(Reference::Remote(target, offset)) => {
                self.type_at(self.units.unit_ref(&target), offset)
            }
            None => {
                tracing::warn!(?value, "unsupported type reference");
                Ok(Type::Base(Base::Void))
            }
        }
    }

    /// Returns the type defined by the DIE at `offset` in `unit`.
    fn type_at(&self, unit: UnitRef<'_, Reader<'data>>, offset: UnitOffset) -> Result<Type, Error> {
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
        unit: UnitRef<'_, Reader<'data>>,
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
