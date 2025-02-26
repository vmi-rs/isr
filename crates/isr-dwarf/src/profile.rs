use std::{borrow::Cow, fs::File, io::Read};

use gimli::RunTimeEndian;
use isr_core::{types::Types, Profile, Symbols};
use object::{Endianness, Object as _};

use super::{
    symbols::SystemMapSymbols as _,
    types::{DwarfCache, DwarfTypes as _},
    Error,
};

pub fn create_profile<F, E>(
    kernel_file: File,
    mut systemmap_file: File,
    serialize: F,
) -> Result<(), Error>
where
    F: FnOnce(&Profile) -> Result<(), E>,
    E: std::error::Error + Send + Sync + 'static,
{
    let kernel_mmap = unsafe { memmap2::Mmap::map(&kernel_file)? };
    let object = object::File::parse(&*kernel_mmap)?;
    let endian = match object.endianness() {
        Endianness::Little => RunTimeEndian::Little,
        Endianness::Big => RunTimeEndian::Big,
    };

    let dwarf_sections = super::_gimli::load_dwarf_sections(&object)?;
    let dwarf = super::_gimli::load_dwarf(&dwarf_sections, endian);

    let mut types = Types::default();

    tracing::debug!("collecting types");
    let mut iter = dwarf.units();
    let mut unit_len = 0;
    while iter.next()?.is_some() {
        unit_len += 1;
    }

    let mut cache = DwarfCache::new();
    let mut iter = dwarf.units();
    let mut unit_idx = 0;
    while let Some(header) = iter.next()? {
        unit_idx += 1;

        tracing::debug!("collecting types: {unit_idx}/{unit_len}");

        let unit = dwarf.unit(header)?;
        let unit_ref = unit.unit_ref(&dwarf);
        types.add(&unit_ref, &mut cache)?;
    }

    tracing::debug!("collecting symbols");
    let mut systemmap = String::new();
    systemmap_file.read_to_string(&mut systemmap)?;
    let symbols = Symbols::parse(&systemmap)?;

    tracing::debug!("writing profile");
    let profile = Profile::new(Cow::Borrowed("Amd64"), symbols, types);

    serialize(&profile).map_err(|err| Error::Serialize(err.into()))?;

    Ok(())
}
