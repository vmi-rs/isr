//! Compilation unit index for cross-unit DIE references.

use std::{cell::RefCell, rc::Rc};

use gimli::{
    AbbreviationsCacheStrategy, AttributeValue, Dwarf, Error, Unit, UnitHeader, UnitOffset, UnitRef,
};

use super::_gimli::Reader;

/// All compilation units of a DWARF file, ordered by their `.debug_info`
/// offset.
///
/// Linkers and `dsymutil` deduplicate types across units and refer to the
/// surviving copy through `DW_FORM_ref_addr`. Resolving such a reference
/// requires the unit that owns the target DIE, because its string offsets,
/// address size and line program differ from those of the referencing unit.
///
/// Only the unit headers are kept. A unit, with its abbreviations and line
/// program, is parsed when it is requested, so large files do not hold every
/// parsed unit in memory at once. The unit targeted by the most recent
/// cross-unit reference is cached, because consecutive references tend to
/// point into the same unit.
pub struct DwarfUnits<'data> {
    /// DWARF sections, with abbreviation tables shared by several units
    /// cached.
    dwarf: Dwarf<Reader<'data>>,

    /// Unit headers in section order.
    headers: Vec<UnitHeader<Reader<'data>>>,

    /// Index into `headers` and the parsed unit of the most recent
    /// cross-unit reference target.
    target: RefCell<Option<(usize, Rc<Unit<Reader<'data>>>)>>,
}

/// Target of a DIE reference attribute.
pub enum Reference<'data> {
    /// DIE in the referencing unit.
    Local(UnitOffset),

    /// DIE in another unit.
    Remote(Rc<Unit<Reader<'data>>>, UnitOffset),
}

impl<'data> DwarfUnits<'data> {
    /// Reads the headers of every unit in `dwarf`.
    pub fn new(mut dwarf: Dwarf<Reader<'data>>) -> Result<Self, Error> {
        dwarf.populate_abbreviations_cache(AbbreviationsCacheStrategy::Duplicates);

        let mut headers = Vec::new();
        let mut iter = dwarf.units();
        while let Some(header) = iter.next()? {
            headers.push(header);
        }

        Ok(Self {
            dwarf,
            headers,
            target: RefCell::new(None),
        })
    }

    /// Returns the number of units.
    pub fn len(&self) -> usize {
        self.headers.len()
    }

    /// Returns `true` if the file contains no units.
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    /// Returns the unit at `index` in section order, parsing it unless it is
    /// the cached target of a cross-unit reference.
    ///
    /// # Panics
    ///
    /// Panics if `index` is not less than [`Self::len`].
    pub fn unit(&self, index: usize) -> Result<Rc<Unit<Reader<'data>>>, Error> {
        if let Some((cached, unit)) = &*self.target.borrow()
            && *cached == index
        {
            return Ok(Rc::clone(unit));
        }

        Ok(Rc::new(self.dwarf.unit(self.headers[index].clone())?))
    }

    /// Returns a reference to `unit` that can read its attributes.
    pub fn unit_ref<'a>(&'a self, unit: &'a Unit<Reader<'data>>) -> UnitRef<'a, Reader<'data>> {
        unit.unit_ref(&self.dwarf)
    }

    /// Resolves a DIE reference attribute found in `unit`.
    ///
    /// Returns `None` for values that are not `.debug_info` references, and
    /// for references pointing outside of every unit.
    pub fn resolve(
        &self,
        unit: UnitRef<'_, Reader<'data>>,
        value: AttributeValue<Reader<'data>>,
    ) -> Result<Option<Reference<'data>>, Error> {
        let offset = match value {
            AttributeValue::UnitRef(offset) => return Ok(Some(Reference::Local(offset))),
            AttributeValue::DebugInfoRef(offset) => offset,
            _ => return Ok(None),
        };

        if let Some(unit_offset) = offset.to_unit_offset(&unit.header) {
            return Ok(Some(Reference::Local(unit_offset)));
        }

        if let Some((_, target)) = &*self.target.borrow()
            && let Some(unit_offset) = offset.to_unit_offset(&target.header)
        {
            return Ok(Some(Reference::Remote(Rc::clone(target), unit_offset)));
        }

        let index = match self
            .headers
            .partition_point(|header| header.offset().0 <= offset.0)
            .checked_sub(1)
        {
            Some(index) => index,
            None => return Ok(None),
        };

        let header = &self.headers[index];
        let unit_offset = match offset.to_unit_offset(header) {
            Some(unit_offset) => unit_offset,
            None => return Ok(None),
        };

        let target = Rc::new(self.dwarf.unit(header.clone())?);
        *self.target.borrow_mut() = Some((index, Rc::clone(&target)));
        Ok(Some(Reference::Remote(target, unit_offset)))
    }
}
