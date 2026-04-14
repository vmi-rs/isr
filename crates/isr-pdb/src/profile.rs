use std::fs::File;

use isr_core::schema::{Architecture, Profile};
use pdb::{MachineType, PDB};

use super::{Error, symbols::PdbSymbols as _, types::PdbTypes as _};

/// Parses `pdb_file` and hands the resulting [`Profile`] to `serialize`.
pub fn create_profile<F, E>(pdb_file: File, serialize: F) -> Result<(), Error>
where
    F: FnOnce(&Profile) -> Result<(), E>,
    E: std::error::Error + Send + Sync + 'static,
{
    let mut pdb = PDB::open(pdb_file)?;
    let mut profile = Profile::default();

    tracing::debug!("collecting debug information");
    let dbi = pdb.debug_information()?;
    profile.architecture = match dbi.machine_type()? {
        MachineType::X86 => Architecture::X86,
        MachineType::Amd64 => Architecture::Amd64,
        MachineType::Arm => Architecture::Arm32,
        MachineType::Arm64 => Architecture::Arm64,
        _ => {
            tracing::warn!(
                machine_type = ?dbi.machine_type()?,
                "unsupported machine type, defaulting to unknown architecture"
            );
            Architecture::Unknown
        }
    };
    tracing::debug!(architecture = ?profile.architecture);

    tracing::debug!("collecting symbols");
    let address_map = pdb.address_map()?;
    let symbol_table = pdb.global_symbols()?;
    profile.parse_symbols(address_map, symbol_table.iter())?;

    tracing::debug!("collecting types");
    let tpi = pdb.type_information()?;
    profile.parse_types(tpi.finder(), tpi.iter())?;

    tracing::debug!("writing profile");
    serialize(&profile).map_err(|err| Error::Serialization(err.into()))?;

    Ok(())
}
