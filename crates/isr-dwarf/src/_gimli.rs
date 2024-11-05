use std::borrow::Cow;

use gimli::{
    Attribute, AttributeValue, DebuggingInformationEntry, DwAte, Dwarf, DwarfSections, EndianSlice,
    EntriesTree, Error, Reader as _, RelocateReader, RunTimeEndian, UnitRef,
};

// This is a simple wrapper around `object::read::RelocationMap` that implements
// `gimli::read::Relocate` for use with `gimli::RelocateReader`.
// You only need this if you are parsing relocatable object files.
#[derive(Debug, Default)]
pub struct RelocationMap(pub object::read::RelocationMap);

impl gimli::read::Relocate for &'_ RelocationMap {
    fn relocate_address(&self, offset: usize, value: u64) -> Result<u64, Error> {
        Ok(self.0.relocate(offset as u64, value))
    }

    fn relocate_offset(&self, offset: usize, value: usize) -> Result<usize, Error> {
        <usize as gimli::ReaderOffset>::from_u64(self.0.relocate(offset as u64, value as u64))
    }
}

// The section data that will be stored in `DwarfSections` and `DwarfPackageSections`.
#[derive(Default)]
pub struct Section<'data> {
    data: Cow<'data, [u8]>,
    relocations: RelocationMap,
}

// The reader type that will be stored in `Dwarf` and `DwarfPackage`.
// If you don't need relocations, you can use `gimli::EndianSlice` directly.
pub type Reader<'data> = RelocateReader<EndianSlice<'data, RunTimeEndian>, &'data RelocationMap>;

pub trait DebuggingInformationEntryExt<'data> {
    fn name(&self, unit: &UnitRef<Reader<'data>>) -> Result<Option<String>, Error>;
    fn type_<'a>(
        &self,
        unit: &'a UnitRef<'a, Reader<'data>>,
    ) -> Result<Option<EntriesTree<'a, 'a, Reader<'data>>>, Error>;
    fn decl_file(&self, unit: &UnitRef<Reader<'data>>) -> Result<Option<String>, Error>;
    fn decl_file_index(&self) -> Result<Option<u64>, Error>;
    fn decl_line(&self) -> Result<Option<u64>, Error>;
    fn decl_column(&self) -> Result<Option<u64>, Error>;
    fn bit_size(&self) -> Result<Option<u64>, Error>;
    fn byte_size(&self) -> Result<Option<u64>, Error>;
    fn count(&self) -> Result<Option<u64>, Error>;
    fn data_bit_offset(&self) -> Result<Option<u64>, Error>;
    fn data_member_location(&self) -> Result<Option<u64>, Error>;
    fn declaration(&self) -> Result<Option<bool>, Error>;
    fn encoding(&self) -> Result<Option<DwAte>, Error>;
    fn upper_bound(&self) -> Result<Option<u64>, Error>;
}

impl<'data> DebuggingInformationEntryExt<'data>
    for DebuggingInformationEntry<'_, '_, Reader<'data>>
{
    fn name(&self, unit: &UnitRef<Reader<'data>>) -> Result<Option<String>, Error> {
        match self.attr(gimli::DW_AT_name)?.as_ref().map(Attribute::value) {
            Some(name) => Ok(Some(unit.attr_string(name)?.to_string_lossy()?.to_string())),
            None => Ok(None),
        }
    }

    fn type_<'a>(
        &self,
        unit: &'a UnitRef<Reader<'data>>,
    ) -> Result<Option<EntriesTree<'a, 'a, Reader<'data>>>, Error> {
        match self.attr(gimli::DW_AT_type)?.as_ref().map(Attribute::value) {
            Some(AttributeValue::UnitRef(offset)) => Ok(Some(unit.entries_tree(Some(offset))?)),
            _ => Ok(None),
        }
    }

    fn decl_file(&self, unit: &UnitRef<Reader<'data>>) -> Result<Option<String>, Error> {
        match self.decl_file_index()? {
            Some(file_index) => {
                let header = match unit.line_program {
                    Some(ref program) => program.header(),
                    None => return Ok(None),
                };
                let file = match header.file(file_index) {
                    Some(file) => file,
                    None => return Ok(None),
                };

                Ok(Some(
                    unit.attr_string(file.path_name())?
                        .to_string_lossy()?
                        .to_string(),
                ))
            }
            _ => Ok(None),
        }
    }

    fn decl_file_index(&self) -> Result<Option<u64>, Error> {
        match self
            .attr(gimli::DW_AT_decl_file)?
            .as_ref()
            .map(Attribute::value)
        {
            Some(AttributeValue::FileIndex(file)) => Ok(Some(file)),
            _ => Ok(None),
        }
    }

    fn decl_line(&self) -> Result<Option<u64>, Error> {
        Ok(self
            .attr(gimli::DW_AT_decl_line)?
            .as_ref()
            .and_then(Attribute::udata_value))
    }

    fn decl_column(&self) -> Result<Option<u64>, Error> {
        Ok(self
            .attr(gimli::DW_AT_decl_column)?
            .as_ref()
            .and_then(Attribute::udata_value))
    }

    fn bit_size(&self) -> Result<Option<u64>, Error> {
        Ok(self
            .attr(gimli::DW_AT_bit_size)?
            .as_ref()
            .and_then(Attribute::udata_value))
    }

    fn byte_size(&self) -> Result<Option<u64>, Error> {
        Ok(self
            .attr(gimli::DW_AT_byte_size)?
            .as_ref()
            .and_then(Attribute::udata_value))
    }

    fn count(&self) -> Result<Option<u64>, Error> {
        Ok(self
            .attr(gimli::DW_AT_count)?
            .as_ref()
            .and_then(Attribute::udata_value))
    }

    fn data_bit_offset(&self) -> Result<Option<u64>, Error> {
        Ok(self
            .attr(gimli::DW_AT_data_bit_offset)?
            .as_ref()
            .and_then(Attribute::udata_value))
    }

    fn data_member_location(&self) -> Result<Option<u64>, Error> {
        Ok(self
            .attr(gimli::DW_AT_data_member_location)?
            .as_ref()
            .and_then(Attribute::udata_value))
    }

    fn declaration(&self) -> Result<Option<bool>, Error> {
        match self
            .attr(gimli::DW_AT_declaration)?
            .as_ref()
            .map(Attribute::value)
        {
            Some(AttributeValue::Flag(flag)) => Ok(Some(flag)),
            _ => Ok(Some(false)),
        }
    }

    fn encoding(&self) -> Result<Option<DwAte>, Error> {
        match self
            .attr(gimli::DW_AT_encoding)?
            .as_ref()
            .map(Attribute::value)
        {
            Some(AttributeValue::Encoding(encoding)) => Ok(Some(encoding)),
            _ => Ok(None),
        }
    }

    fn upper_bound(&self) -> Result<Option<u64>, Error> {
        Ok(self
            .attr(gimli::DW_AT_upper_bound)?
            .as_ref()
            .and_then(Attribute::udata_value))
    }
}

pub fn load_dwarf_sections<'data>(
    object: &object::File<'data>,
) -> Result<DwarfSections<Section<'data>>, object::Error> {
    use object::{Object as _, ObjectSection};

    // Load a `Section` that may own its data.
    fn load_section<'data>(
        object: &object::File<'data>,
        name: &str,
    ) -> Result<Section<'data>, object::Error> {
        Ok(match object.section_by_name(name) {
            Some(section) => Section {
                data: section.uncompressed_data()?,
                relocations: section.relocation_map().map(RelocationMap)?,
            },
            None => Default::default(),
        })
    }

    // Load all of the sections.
    DwarfSections::load(|id| load_section(object, id.name()))
}

pub fn load_dwarf<'data>(
    dwarf_sections: &'data DwarfSections<Section<'data>>,
    endian: RunTimeEndian,
) -> Dwarf<Reader<'data>> {
    // Borrow a `Section` to create a `Reader`.
    fn borrow_section<'data>(
        section: &'data Section<'data>,
        endian: RunTimeEndian,
    ) -> Reader<'data> {
        let slice = EndianSlice::new(Cow::as_ref(&section.data), endian);
        RelocateReader::new(slice, &section.relocations)
    }

    // Create `Reader`s for all of the sections and do preliminary parsing.
    // Alternatively, we could have used `Dwarf::load` with an owned type such as `EndianRcSlice`.
    dwarf_sections.borrow(|section| borrow_section(section, endian))
}
