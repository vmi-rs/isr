//! Implementation details exposed for macro expansion and test setup.

mod fixtures;

pub use isr_core::Profile;

pub use self::fixtures::ntkrnlmp_profile;
pub use super::{offsets::IntoField, profile::ProfileExt, symbols::IntoSymbol};
