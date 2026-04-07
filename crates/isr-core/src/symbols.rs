use std::borrow::Cow;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Symbols.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Symbols<'p>(
    #[serde(borrow, with = "crate::serde_cow_map")]
    pub IndexMap<Cow<'p, str>, u64>,
);
