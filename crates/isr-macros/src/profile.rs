use isr_core::{Profile, types::Type};

use crate::{Bitfield, Error, Field, offsets::FieldDescriptor, symbols::SymbolDescriptor};

pub trait ProfileExt {
    fn find_field(&self, type_name: &str, field_name: &str) -> Option<Field>;
    fn find_bitfield(&self, type_name: &str, field_name: &str) -> Option<Bitfield>;
    fn find_symbol_descriptor(&self, symbol_name: &str) -> Result<SymbolDescriptor, Error>;
    fn find_field_descriptor(
        &self,
        type_name: &str,
        field_name: &str,
    ) -> Result<FieldDescriptor, Error>;
}

impl ProfileExt for Profile<'_> {
    fn find_field(&self, type_name: &str, field_name: &str) -> Option<Field> {
        let udt = self.find_struct(type_name)?;

        if let Some(field) = udt.fields.get(field_name) {
            return Some(Field {
                offset: field.offset,
                size: self.type_size(&field.type_)?,
            });
        }

        for field in udt.fields.values() {
            let udt = match &field.type_ {
                Type::Struct(udt) => udt,
                _ => continue,
            };

            if let Some(child) = self.find_field(&udt.name, field_name) {
                return Some(Field {
                    offset: field.offset + child.offset,
                    size: child.size,
                });
            }
        }

        None
    }

    fn find_bitfield(&self, type_name: &str, field_name: &str) -> Option<Bitfield> {
        let udt = self.find_struct(type_name)?;

        if let Some(field) = udt.fields.get(field_name) {
            if let Type::Bitfield(bitfield) = &field.type_ {
                return Some(Bitfield {
                    field: Field {
                        offset: field.offset,
                        size: self.type_size(&field.type_)?,
                    },
                    bit_position: bitfield.bit_position,
                    bit_length: bitfield.bit_length,
                });
            }
        }

        for field in udt.fields.values() {
            let udt = match &field.type_ {
                Type::Struct(udt) => udt,
                _ => continue,
            };

            if let Some(child) = self.find_bitfield(&udt.name, field_name) {
                return Some(Bitfield {
                    field: Field {
                        offset: field.offset + child.offset,
                        size: child.size,
                    },
                    bit_position: child.bit_position,
                    bit_length: child.bit_length,
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

        if let Some(field) = udt.fields.get(field_name) {
            return Ok(match &field.type_ {
                Type::Bitfield(bitfield) => FieldDescriptor::Bitfield(Bitfield {
                    field: Field {
                        offset: field.offset,
                        size: match self.type_size(&field.type_) {
                            Some(size) => size,
                            None => {
                                return Err(Error::field_not_found(type_name, field_name));
                            }
                        },
                    },
                    bit_position: bitfield.bit_position,
                    bit_length: bitfield.bit_length,
                }),
                _ => FieldDescriptor::Field(Field {
                    offset: field.offset,
                    size: match self.type_size(&field.type_) {
                        Some(size) => size,
                        None => {
                            return Err(Error::field_not_found(type_name, field_name));
                        }
                    },
                }),
            });
        }

        for field in udt.fields.values() {
            let udt = match &field.type_ {
                Type::Struct(udt) => udt,
                _ => continue,
            };

            if let Ok(child) = self.find_field_descriptor(&udt.name, field_name) {
                return Ok(match child {
                    FieldDescriptor::Field(child) => FieldDescriptor::Field(Field {
                        offset: field.offset + child.offset,
                        size: child.size,
                    }),
                    FieldDescriptor::Bitfield(child) => FieldDescriptor::Bitfield(Bitfield {
                        field: Field {
                            offset: field.offset + child.offset,
                            size: child.size,
                        },
                        bit_position: child.bit_position,
                        bit_length: child.bit_length,
                    }),
                });
            }
        }

        Err(Error::field_not_found(type_name, field_name))
    }
}
