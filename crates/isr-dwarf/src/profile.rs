use std::{fs::File, io::Read};

use gimli::RunTimeEndian;
use isr_core::schema::{Architecture, Profile};
use object::{Endianness, Object as _};

use super::{Error, symbols::SystemMapSymbols as _, types::DwarfTypes as _, units::DwarfUnits};

/// Parses `kernel_file` (DWARF) + `systemmap_file` and hands the resulting
/// [`Profile`] to `serialize`.
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

    let mut profile = Profile {
        architecture: Architecture::Amd64,
        ..Default::default()
    };

    add_types(&mut profile, &object)?;

    tracing::debug!("collecting symbols");
    let mut systemmap = String::new();
    systemmap_file.read_to_string(&mut systemmap)?;
    profile.parse_symbols(&systemmap)?;

    tracing::debug!("writing profile");
    serialize(&profile).map_err(|err| Error::Serialization(err.into()))?;

    Ok(())
}

/// Adds the types described by the DWARF sections of `object`.
fn add_types(profile: &mut Profile, object: &object::File<'_>) -> Result<(), Error> {
    let endian = match object.endianness() {
        Endianness::Little => RunTimeEndian::Little,
        Endianness::Big => RunTimeEndian::Big,
    };

    let dwarf_sections = super::_gimli::load_dwarf_sections(object)?;
    let dwarf = super::_gimli::load_dwarf(&dwarf_sections, endian);

    tracing::debug!("parsing units");
    let units = DwarfUnits::new(dwarf)?;

    tracing::debug!("collecting types");
    profile.add_types(&units)?;

    Ok(())
}
