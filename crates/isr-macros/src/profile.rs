use isr_core::{Profile, Type};

use crate::{Bitfield, Error, Field, offsets::FieldDescriptor, symbols::SymbolDescriptor};

/// Convenience lookups layered on top of [`Profile`].
pub trait ProfileExt {
    /// Returns the field named `field_name` in `type_name`, recursing into
    /// nested struct-typed fields if the name is not found directly.
    fn find_field(&self, type_name: &str, field_name: &str) -> Option<Field>;

    /// Returns the bitfield named `field_name` in `type_name`, recursing into
    /// nested struct-typed fields if the name is not found directly.
    fn find_bitfield(&self, type_name: &str, field_name: &str) -> Option<Bitfield>;

    /// Returns the symbol named `symbol_name`, or an error if absent.
    fn find_symbol_descriptor(&self, symbol_name: &str) -> Result<SymbolDescriptor, Error>;

    /// Returns a descriptor for the field named `field_name` in `type_name`,
    /// or an error if either name is absent. Recurses into nested structs.
    fn find_field_descriptor(
        &self,
        type_name: &str,
        field_name: &str,
    ) -> Result<FieldDescriptor, Error>;
}

impl ProfileExt for Profile<'_> {
    fn find_field(&self, type_name: &str, field_name: &str) -> Option<Field> {
        let udt = self.find_struct(type_name)?;

        if let Some(field) = udt.field(field_name) {
            return Some(Field {
                offset: field.offset(),
                size: self.type_size(field.ty())?,
            });
        }

        for field in udt.fields() {
            let udt = match field.ty() {
                Type::Struct(udt) => udt,
                _ => continue,
            };

            if let Some(child) = self.find_field(udt.name(), field_name) {
                return Some(Field {
                    offset: field.offset() + child.offset(),
                    size: child.size(),
                });
            }
        }

        None
    }

    fn find_bitfield(&self, type_name: &str, field_name: &str) -> Option<Bitfield> {
        let udt = self.find_struct(type_name)?;

        if let Some(field) = udt.field(field_name)
            && let Type::Bitfield(bitfield) = field.ty()
        {
            return Some(Bitfield {
                field: Field {
                    offset: field.offset(),
                    size: self.type_size(field.ty())?,
                },
                bit_position: bitfield.bit_position(),
                bit_length: bitfield.bit_length(),
            });
        }

        for field in udt.fields() {
            let udt = match field.ty() {
                Type::Struct(udt) => udt,
                _ => continue,
            };

            if let Some(child) = self.find_bitfield(udt.name(), field_name) {
                return Some(Bitfield {
                    field: Field {
                        offset: field.offset() + child.offset(),
                        size: child.size(),
                    },
                    bit_position: child.bit_position(),
                    bit_length: child.bit_length(),
                });
            };
        }

        None
    }

    fn find_symbol_descriptor(&self, symbol_name: &str) -> Result<SymbolDescriptor, Error> {
        match self.find_symbol(symbol_name) {
            Some(offset) => Ok(SymbolDescriptor { offset }),
            None => Err(Error::symbol_not_found(symbol_name)),
        }
    }

    fn find_field_descriptor(
        &self,
        type_name: &str,
        field_name: &str,
    ) -> Result<FieldDescriptor, Error> {
        let udt = match self.find_struct(type_name) {
            Some(udt) => udt,
            None => return Err(Error::type_not_found(type_name)),
        };

        if let Some(field) = udt.field(field_name) {
            match field.ty() {
                Type::Bitfield(bitfield) => {
                    return Ok(FieldDescriptor::Bitfield(Bitfield {
                        field: Field {
                            offset: field.offset(),
                            size: match self.type_size(field.ty()) {
                                Some(size) => size,
                                None => {
                                    return Err(Error::field_not_found(type_name, field_name));
                                }
                            },
                        },
                        bit_position: bitfield.bit_position(),
                        bit_length: bitfield.bit_length(),
                    }));
                }
                _ => {
                    return Ok(FieldDescriptor::Field(Field {
                        offset: field.offset(),
                        size: match self.type_size(field.ty()) {
                            Some(size) => size,
                            None => {
                                return Err(Error::field_not_found(type_name, field_name));
                            }
                        },
                    }));
                }
            }
        }

        for field in udt.fields() {
            let udt = match field.ty() {
                Type::Struct(udt) => udt,
                _ => continue,
            };

            if let Ok(child) = self.find_field_descriptor(udt.name(), field_name) {
                match child {
                    FieldDescriptor::Field(child) => {
                        return Ok(FieldDescriptor::Field(Field {
                            offset: field.offset() + child.offset(),
                            size: child.size(),
                        }));
                    }
                    FieldDescriptor::Bitfield(child) => {
                        return Ok(FieldDescriptor::Bitfield(Bitfield {
                            field: Field {
                                offset: field.offset() + child.offset(),
                                size: child.size(),
                            },
                            bit_position: child.bit_position(),
                            bit_length: child.bit_length(),
                        }));
                    }
                }
            }
        }

        Err(Error::field_not_found(type_name, field_name))
    }
}
