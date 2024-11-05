use std::borrow::Cow;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Symbols.
#[derive(Debug, Serialize, Deserialize)]
pub struct Symbols<'p>(#[serde(borrow)] pub IndexMap<Cow<'p, str>, u64>);
