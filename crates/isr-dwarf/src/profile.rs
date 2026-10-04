use std::{fs::File, io::Read};

use gimli::RunTimeEndian;
use isr_core::schema::{Architecture, Profile};
use object::{
    BinaryFormat, Endianness, FileKind, Object as _, macho,
    read::macho::{FatArch, MachOFatFile32, MachOFatFile64},
};

use super::{
    Error,
    segments::MachOSegments as _,
    symbols::{MachOSymbols as _, SystemMapSymbols as _},
    types::DwarfTypes as _,
    units::DwarfUnits,
};

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
        architecture: architecture(&object),
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

/// Parses a Mach-O dSYM companion file and hands the resulting [`Profile`]
/// to `serialize`.
///
/// Types come from the DWARF sections, symbols from the Mach-O symbol table
/// and segments from the `LC_SEGMENT_64` load commands. Symbol values and
/// segment addresses are unslid virtual addresses. From a universal binary,
/// the arm64 slice is used, preferring arm64e.
pub fn create_profile_from_dsym<F, E>(dsym_file: File, serialize: F) -> Result<(), Error>
where
    F: FnOnce(&Profile) -> Result<(), E>,
    E: std::error::Error + Send + Sync + 'static,
{
    let dsym_mmap = unsafe { memmap2::Mmap::map(&dsym_file)? };
    let object = object::File::parse(macho_slice(&dsym_mmap)?)?;
    if object.format() != BinaryFormat::MachO {
        return Err(Error::UnsupportedFormat(object.format()));
    }

    let mut profile = Profile {
        architecture: architecture(&object),
        ..Default::default()
    };

    add_types(&mut profile, &object)?;

    tracing::debug!("collecting symbols");
    profile.parse_macho_symbols(&object)?;

    tracing::debug!("collecting segments");
    profile.parse_macho_segments(&object)?;

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

/// Returns the profile architecture of `object`.
fn architecture(object: &object::File<'_>) -> Architecture {
    match object.architecture() {
        object::Architecture::I386 => Architecture::X86,
        object::Architecture::X86_64 => Architecture::Amd64,
        object::Architecture::Arm => Architecture::Arm32,
        object::Architecture::Aarch64 => Architecture::Arm64,
        architecture => {
            tracing::warn!(
                ?architecture,
                "unsupported architecture, defaulting to unknown architecture"
            );
            Architecture::Unknown
        }
    }
}

/// Returns the arm64 slice of a universal binary, preferring arm64e, or
/// `data` itself for any other file.
fn macho_slice(data: &[u8]) -> Result<&[u8], Error> {
    match FileKind::parse(data)? {
        FileKind::MachOFat32 => fat_slice(MachOFatFile32::parse(data)?.arches(), data),
        FileKind::MachOFat64 => fat_slice(MachOFatFile64::parse(data)?.arches(), data),
        _ => Ok(data),
    }
}

/// Selects the arm64 slice from the `arches` of a universal binary.
fn fat_slice<'data>(arches: &[impl FatArch], data: &'data [u8]) -> Result<&'data [u8], Error> {
    let arch = arches
        .iter()
        .filter(|arch| arch.architecture() == object::Architecture::Aarch64)
        .max_by_key(|arch| arch.cpusubtype().id() == macho::CPU_SUBTYPE_ARM64E);

    match arch {
        Some(arch) => Ok(arch.data(data)?),
        None => Err(Error::MissingArm64Slice),
    }
}
