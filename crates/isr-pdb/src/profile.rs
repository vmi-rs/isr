use std::fs::File;

use isr_core::{Profile, Symbols, types::Types};
use pdb::PDB;

use super::{Error, symbols::PdbSymbols as _, types::PdbTypes as _};

pub fn create_profile<F, E>(pdb_file: File, serialize: F) -> Result<(), Error>
where
    F: FnOnce(&Profile) -> Result<(), E>,
    E: std::error::Error + Send + Sync + 'static,
{
    let mut pdb = PDB::open(pdb_file)?;

    tracing::debug!("collecting debug information");
    let dbi = pdb.debug_information()?;
    let architecture = dbi.machine_type()?.to_string().into();
    tracing::debug!("architecture: {architecture}");

    tracing::debug!("collecting symbols");
    let address_map = pdb.address_map()?;
    let symbol_table = pdb.global_symbols()?;
    let symbols = Symbols::parse(address_map, symbol_table.iter())?;

    tracing::debug!("collecting types");
    let tpi = pdb.type_information()?;
    let types = Types::parse(tpi.finder(), tpi.iter())?;

    tracing::debug!("writing profile");
    let profile = Profile::new(architecture, symbols, types);

    serialize(&profile).map_err(|err| Error::Serialize(err.into()))?;

    Ok(())
}
