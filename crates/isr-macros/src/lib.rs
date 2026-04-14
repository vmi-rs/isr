//! [`offsets!`] and [`symbols!`] macros.

mod error;
mod offsets;
mod profile;
mod symbols;

#[doc(hidden)]
#[path = "private/mod.rs"]
pub mod __private;

pub use self::{
    error::Error,
    offsets::{Bitfield, Field},
};
