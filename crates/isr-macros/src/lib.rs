//! [`offsets!`] and [`symbols!`] macros.

mod error;
mod offsets;
mod profile;
mod symbols;

pub mod __private {
    pub use isr_core::Profile;

    pub use super::{offsets::IntoField, profile::ProfileExt, symbols::IntoSymbol};
}

pub use self::{
    error::Error,
    offsets::{Bitfield, Field},
};
