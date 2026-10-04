//! Segment table parsing.

use isr_core::schema::{Profile, Segment};
use object::{Object as _, ObjectSegment as _};

use super::Error;

/// Name of the Mach-O segment holding the DWARF sections of a dSYM.
const DWARF_SEGMENT: &str = "__DWARF";

/// Populates a [`Profile`] with segments from a Mach-O load command table.
pub trait MachOSegments {
    /// Inserts the `LC_SEGMENT_64` segments of `object` into `self`.
    ///
    /// Addresses are the unslid virtual addresses from the load commands.
    /// Empty segments and the `__DWARF` segment of a dSYM are skipped.
    fn parse_macho_segments(&mut self, object: &object::File<'_>) -> Result<(), Error>;
}

impl MachOSegments for Profile {
    fn parse_macho_segments(&mut self, object: &object::File<'_>) -> Result<(), Error> {
        for segment in object.segments() {
            let name = match segment.name()? {
                Some(name) => name,
                None => continue,
            };

            let size = segment.size();
            if size == 0 || name == DWARF_SEGMENT {
                continue;
            }

            let address = segment.address();
            if let Some(previous) = self
                .segments
                .insert(name.to_owned(), Segment { address, size })
            {
                tracing::warn!(
                    name,
                    previous.address,
                    address,
                    "duplicate segment name; overwriting"
                );
            }
        }

        Ok(())
    }
}
