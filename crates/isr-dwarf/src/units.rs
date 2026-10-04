//! Compilation unit index for cross-unit DIE references.

use gimli::{AttributeValue, DebugInfoOffset, Dwarf, Error, Unit, UnitOffset, UnitRef};

use super::_gimli::Reader;

/// All compilation units of a DWARF file, ordered by their `.debug_info`
/// offset.
///
/// Linkers and `dsymutil` deduplicate types across units and refer to the
/// surviving copy through `DW_FORM_ref_addr`. Resolving such a reference
/// requires the unit that owns the target DIE, because its string offsets,
/// address size and line program differ from those of the referencing unit.
pub struct DwarfUnits<'data> {
    dwarf: Dwarf<Reader<'data>>,
    units: Vec<Unit<Reader<'data>>>,
}

impl<'data> DwarfUnits<'data> {
    /// Parses the headers, abbreviations and line programs of every unit in
    /// `dwarf`.
    pub fn new(dwarf: Dwarf<Reader<'data>>) -> Result<Self, Error> {
        let mut units = Vec::new();
        let mut headers = dwarf.units();
        while let Some(header) = headers.next()? {
            units.push(dwarf.unit(header)?);
        }

        Ok(Self { dwarf, units })
    }

    /// Returns the number of units.
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// Returns `true` if the file contains no units.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// Returns the units in section order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = UnitRef<'_, Reader<'data>>> {
        self.units.iter().map(|unit| unit.unit_ref(&self.dwarf))
    }

    /// Resolves a DIE reference attribute found in `unit` to the unit that
    /// owns the target DIE and the offset of that DIE within it.
    ///
    /// Returns `None` for values that are not `.debug_info` references, and
    /// for references pointing outside of every unit.
    pub fn resolve<'u>(
        &'u self,
        unit: UnitRef<'u, Reader<'data>>,
        value: AttributeValue<Reader<'data>>,
    ) -> Option<(UnitRef<'u, Reader<'data>>, UnitOffset)> {
        match value {
            AttributeValue::UnitRef(offset) => Some((unit, offset)),
            AttributeValue::DebugInfoRef(offset) => self.find(offset),
            _ => None,
        }
    }

    /// Finds the unit containing the `.debug_info` section `offset`.
    fn find(&self, offset: DebugInfoOffset) -> Option<(UnitRef<'_, Reader<'data>>, UnitOffset)> {
        let index = self
            .units
            .partition_point(|unit| unit.header.offset().0 <= offset.0)
            .checked_sub(1)?;

        let unit = &self.units[index];
        let unit_offset = offset.to_unit_offset(&unit.header)?;
        Some((unit.unit_ref(&self.dwarf), unit_offset))
    }
}
