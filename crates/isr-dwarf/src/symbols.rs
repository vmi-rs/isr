//! `System.map` symbol parsing.

use isr_core::schema::Profile;

use super::Error;

/// Populates a [`Profile`] with symbols parsed from a `System.map` file.
pub trait SystemMapSymbols<'a> {
    /// Parses `systemmap` and inserts its symbols into `self`.
    ///
    /// Keeps the symbols of the kinds `t` and `T` (text), `d` and `D` (data),
    /// and `r` and `R` (read-only data). A name that occurs more than once
    /// keeps the address of its last occurrence.
    fn parse_symbols(&mut self, systemmap: &'a str) -> Result<(), Error>;
}

impl<'a> SystemMapSymbols<'a> for Profile {
    fn parse_symbols(&mut self, systemmap: &'a str) -> Result<(), Error> {
        for line in systemmap.lines() {
            let mut parts = line.split_whitespace();
            let rva = parts.next().ok_or(Error::InvalidSystemMap)?;
            let kind = parts.next().ok_or(Error::InvalidSystemMap)?;
            let name = parts.next().ok_or(Error::InvalidSystemMap)?;

            if !matches!(kind, "d" | "D" | "r" | "R" | "t" | "T") {
                continue;
            }

            let rva = u64::from_str_radix(rva, 16).map_err(|_| Error::InvalidSystemMap)?;
            self.symbols.insert(name.to_owned(), rva);
        }

        Ok(())
    }
}
