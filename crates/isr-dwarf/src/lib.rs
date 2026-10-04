//! DWARF debugging information parsing and processing.

mod _gimli;
mod error;
mod profile;
pub mod symbols;
pub mod types;
pub mod units;

pub use self::{error::Error, profile::create_profile};
