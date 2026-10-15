//! DWARF function prototype extraction.

use gimli::{DebuggingInformationEntry, EntriesTreeNode, Error, UnitRef};
use indexmap::map::Entry;
use isr_core::schema::{Base, Function, Parameter, Profile, Type};

use super::{
    _gimli::{DebuggingInformationEntryExt as _, Reader},
    types::TypeCollector,
    units::Reference,
};

/// Attributes of a subprogram that a definition may leave to the
/// declaration it completes.
#[derive(Default)]
struct Declaration {
    /// Linkage (mangled) name.
    linkage_name: Option<String>,

    /// Source name.
    name: Option<String>,

    /// Whether the function is visible outside its unit.
    external: bool,

    /// Return type, `None` if the entry does not record one.
    return_type: Option<Type>,
}

impl Declaration {
    /// Fills the attributes this entry lacks from `specification`, the
    /// declaration it completes.
    fn complete(&mut self, specification: Self) {
        if self.linkage_name.is_none() {
            self.linkage_name = specification.linkage_name;
        }

        if self.name.is_none() {
            self.name = specification.name;
        }

        if self.return_type.is_none() {
            self.return_type = specification.return_type;
        }

        self.external |= specification.external;
    }
}

impl<'data> TypeCollector<'_, 'data> {
    /// Adds the prototype of the subprogram `node`, declared in unit or
    /// namespace scope, to `profile`.
    ///
    /// A definition completing a declaration through `DW_AT_specification`,
    /// such as an out-of-line C++ method, takes its name, visibility and
    /// return type from that declaration where it does not repeat them.
    /// Concrete instances of inlined functions, which refer to their
    /// abstract instance through `DW_AT_abstract_origin`, are skipped,
    /// because the abstract instance is a subprogram of its own. Subprograms
    /// without a name and those not visible outside their unit are skipped.
    ///
    /// A definition replaces a declaration of the same function, and
    /// otherwise the copy with more parameters is kept.
    pub(crate) fn add_function(
        &self,
        profile: &mut Profile,
        unit: UnitRef<'_, Reader<'data>>,
        node: EntriesTreeNode<'_, '_, Reader<'data>>,
    ) -> Result<(), Error> {
        let entry = node.entry();
        if entry.attr_value(gimli::DW_AT_abstract_origin).is_some() {
            return Ok(());
        }

        let definition = !entry.declaration()?.unwrap_or(false);

        let mut declaration = self.declaration(unit, entry)?;
        if let Some(value) = entry.attr_value(gimli::DW_AT_specification) {
            let specification = match self.units.resolve(unit, value)? {
                Some(Reference::Local(offset)) => self.declaration(unit, &unit.entry(offset)?)?,
                Some(Reference::Remote(target, offset)) => {
                    let target_unit = self.units.unit_ref(&target);
                    self.declaration(target_unit, &target_unit.entry(offset)?)?
                }
                None => Declaration::default(),
            };

            declaration.complete(specification);
        }

        if !declaration.external {
            return Ok(());
        }

        let name = match declaration.linkage_name.or(declaration.name) {
            Some(name) => name,
            None => return Ok(()),
        };

        let mut parameters = Vec::new();
        let mut variadic = false;
        let mut children = node.children();
        while let Some(child) = children.next()? {
            let child = child.entry();
            match child.tag() {
                gimli::DW_TAG_formal_parameter => parameters.push(Parameter {
                    name: child.name(&unit)?,
                    ty: self.attr_type(unit, child)?,
                }),
                gimli::DW_TAG_unspecified_parameters => variadic = true,
                _ => (),
            }
        }

        let function = Function {
            return_type: match declaration.return_type {
                Some(return_type) => return_type,
                None => Type::Base(Base::Void),
            },
            parameters,
            variadic,
        };

        let mut defined_functions = self.defined_functions.borrow_mut();
        match profile.functions.entry(name) {
            Entry::Vacant(entry) => {
                if definition {
                    defined_functions.insert(entry.key().clone());
                }

                entry.insert(function);
            }
            Entry::Occupied(mut entry) => {
                let previous_definition = defined_functions.contains(entry.key());
                let replace = match (definition, previous_definition) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => function.parameters.len() > entry.get().parameters.len(),
                };

                if replace {
                    if definition && !previous_definition {
                        defined_functions.insert(entry.key().clone());
                    }

                    *entry.get_mut() = function;
                }
            }
        }

        Ok(())
    }

    /// Returns the attributes of the subprogram `entry` of `unit` without
    /// following `DW_AT_specification`.
    fn declaration(
        &self,
        unit: UnitRef<'_, Reader<'data>>,
        entry: &DebuggingInformationEntry<Reader<'data>>,
    ) -> Result<Declaration, Error> {
        let return_type = match entry.attr_value(gimli::DW_AT_type) {
            Some(_) => Some(self.attr_type(unit, entry)?),
            None => None,
        };

        Ok(Declaration {
            linkage_name: entry.linkage_name(&unit)?,
            name: entry.name(&unit)?,
            external: entry.external(),
            return_type,
        })
    }
}
