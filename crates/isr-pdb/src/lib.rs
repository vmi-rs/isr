//! PDB file format parser.

mod error;
mod profile;
mod symbols;
mod types;

pub use self::{error::Error, profile::create_profile};
