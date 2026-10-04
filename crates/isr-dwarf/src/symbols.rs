//! Symbol table parsing.

use std::collections::HashSet;

use indexmap::{IndexMap, map::Entry};
use isr_core::schema::Profile;
use object::{Object as _, ObjectSymbol as _, SymbolSection};

use super::Error;

/// Populates a [`Profile`] with symbols parsed from a `System.map` file.
pub trait SystemMapSymbols<'a> {
    /// Parses `systemmap` and inserts its symbols into `self`.
    fn parse_symbols(&mut self, systemmap: &'a str) -> Result<(), Error>;
}

impl<'a> SystemMapSymbols<'a> for Profile {
    fn parse_symbols(&mut self, systemmap: &'a str) -> Result<(), Error> {
        for line in systemmap.lines() {
            let mut parts = line.split_whitespace();
            let rva = parts.next().ok_or(Error::InvalidSystemMap)?;
            let kind = parts.next().ok_or(Error::InvalidSystemMap)?;
            let name = parts.next().ok_or(Error::InvalidSystemMap)?;

            if !matches!(kind, "d" | "D" | "t" | "T") {
                continue;
            }

            let rva = u64::from_str_radix(rva, 16).map_err(|_| Error::InvalidSystemMap)?;
            self.symbols.insert(name.to_owned(), rva);
        }

        Ok(())
    }
}

/// Populates a [`Profile`] with symbols from a Mach-O symbol table.
pub trait MachOSymbols {
    /// Inserts the defined symbols of `object` into `self`.
    ///
    /// Symbol values are the unslid virtual addresses stored in the symbol
    /// table. Names lose exactly one leading underscore, the C symbol prefix
    /// of Mach-O. Debugger (STAB) entries, undefined and common symbols are
    /// skipped. When several symbols share a name, the first external symbol
    /// wins, otherwise the first symbol does.
    fn parse_macho_symbols(&mut self, object: &object::File<'_>) -> Result<(), Error>;
}

impl MachOSymbols for Profile {
    fn parse_macho_symbols(&mut self, object: &object::File<'_>) -> Result<(), Error> {
        let mut globals = HashSet::new();

        // `object` skips STAB entries while iterating the symbol table.
        for symbol in object.symbols() {
            if !matches!(
                symbol.section(),
                SymbolSection::Section(_) | SymbolSection::Absolute
            ) {
                continue;
            }

            let name = match symbol.name() {
                Ok(name) => name,
                Err(err) => {
                    tracing::warn!(index = symbol.index().0, %err, "invalid symbol name");
                    continue;
                }
            };

            let name = name.strip_prefix('_').unwrap_or(name);
            insert_symbol(
                &mut self.symbols,
                &mut globals,
                name,
                symbol.address(),
                symbol.is_global(),
            );
        }

        Ok(())
    }
}

/// Inserts a symbol, preferring the first global definition of a name over
/// local ones.
///
/// `globals` holds the names whose current value comes from a global symbol.
fn insert_symbol(
    symbols: &mut IndexMap<String, u64>,
    globals: &mut HashSet<String>,
    name: &str,
    address: u64,
    global: bool,
) {
    match symbols.entry(name.to_owned()) {
        Entry::Vacant(entry) => {
            entry.insert(address);
            if global {
                globals.insert(name.to_owned());
            }
        }
        Entry::Occupied(mut entry) => {
            if global && !globals.contains(name) {
                tracing::debug!(
                    name,
                    previous = *entry.get(),
                    address,
                    "duplicate symbol; preferring global symbol"
                );

                entry.insert(address);
                globals.insert(name.to_owned());
            }
            else if *entry.get() != address {
                tracing::debug!(
                    name,
                    kept = *entry.get(),
                    address,
                    "duplicate symbol; keeping first"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use indexmap::IndexMap;

    use super::insert_symbol;

    #[test]
    fn global_symbol_replaces_earlier_local() {
        let mut symbols = IndexMap::new();
        let mut globals = HashSet::new();

        insert_symbol(&mut symbols, &mut globals, "counter", 0x1000, false);
        insert_symbol(&mut symbols, &mut globals, "counter", 0x2000, true);
        insert_symbol(&mut symbols, &mut globals, "counter", 0x3000, false);
        insert_symbol(&mut symbols, &mut globals, "counter", 0x4000, true);

        assert_eq!(symbols["counter"], 0x2000);
    }

    #[test]
    fn first_local_symbol_wins_without_global() {
        let mut symbols = IndexMap::new();
        let mut globals = HashSet::new();

        insert_symbol(&mut symbols, &mut globals, "helper", 0x1000, false);
        insert_symbol(&mut symbols, &mut globals, "helper", 0x2000, false);

        assert_eq!(symbols["helper"], 0x1000);
    }
}
