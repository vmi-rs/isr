use isr_core::schema::Profile;
use pdb::{AddressMap, Error, FallibleIterator, SymbolData, SymbolIter};

pub trait PdbSymbols<'p> {
    fn parse_symbols<'s>(
        &mut self,
        address_map: AddressMap<'s>,
        symbol_iter: SymbolIter<'p>,
    ) -> Result<(), Error>;
}

impl<'p> PdbSymbols<'p> for Profile {
    fn parse_symbols<'s>(
        &mut self,
        address_map: AddressMap<'s>,
        mut symbol_iter: SymbolIter<'p>,
    ) -> Result<(), Error> {
        while let Some(symbol) = symbol_iter.next()? {
            if let SymbolData::Public(data) = symbol.parse()? {
                let name = match std::str::from_utf8(data.name.as_bytes()) {
                    Ok(name) => name,
                    Err(_) => {
                        tracing::warn!(
                            name = %data.name,
                            "failed to convert symbol name to UTF-8"
                        );
                        continue;
                    }
                };

                let rva = match data.offset.to_rva(&address_map) {
                    Some(rva) => rva,
                    None => {
                        tracing::trace!(
                            name = %name,
                            rva = ?data.offset,
                            "failed to convert offset to RVA"
                        );
                        continue;
                    }
                };

                if let Some(v) = self.symbols.insert(name.to_owned(), u32::from(rva) as _) {
                    tracing::warn!(
                        name = %name,
                        rva1 = ?v,
                        rva2 = ?rva,
                        "duplicate symbol name with different RVAs"
                    );
                }
            }
        }

        Ok(())
    }
}
