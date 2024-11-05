use indexmap::IndexMap;
use isr_core::Symbols;

use super::Error;

pub trait SystemMapSymbols<'a> {
    fn parse(systemmap: &'a str) -> Result<Symbols<'a>, Error>;
}

impl<'a> SystemMapSymbols<'a> for Symbols<'a> {
    fn parse(systemmap: &'a str) -> Result<Symbols<'a>, Error> {
        let mut result = IndexMap::new();

        for line in systemmap.lines() {
            let mut parts = line.split_whitespace();
            let rva = parts.next().ok_or(Error::InvalidSystemMap)?;
            let kind = parts.next().ok_or(Error::InvalidSystemMap)?;
            let name = parts.next().ok_or(Error::InvalidSystemMap)?;

            if !matches!(kind, "d" | "D" | "t" | "T") {
                continue;
            }

            let rva = u64::from_str_radix(rva, 16).map_err(|_| Error::InvalidSystemMap)?;
            result.insert(name.into(), rva);
        }

        Ok(Self(result))
    }
}
