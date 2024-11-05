use indexmap::IndexMap;
use isr_core::Symbols;
use pdb::{AddressMap, Error, FallibleIterator, SymbolData, SymbolIter};

pub trait PdbSymbols<'p> {
    fn parse<'s>(
        address_map: AddressMap<'s>,
        symbol_iter: SymbolIter<'p>,
    ) -> Result<Symbols<'p>, Error>;
}

impl<'p> PdbSymbols<'p> for Symbols<'p> {
    fn parse<'s>(
        address_map: AddressMap<'s>,
        symbol_iter: SymbolIter<'p>,
    ) -> Result<Symbols<'p>, Error> {
        let mut result = IndexMap::new();

        let mut symbol_iter = symbol_iter;
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
                        tracing::warn!(
                            name = %name,
                            rva = ?data.offset,
                            "failed to convert offset to RVA"
                        );
                        continue;
                    }
                };

                result.insert(name.into(), u32::from(rva).into());
            }
        }

        Ok(Self(result))
    }
}
