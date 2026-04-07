//! Serde helper for `IndexMap<Cow<str>, V>` that borrows map keys
//! directly from the input data when the deserializer supports
//! zero-copy access.
//!
//! Serde's generic `Cow<T>` deserializer always produces `Cow::Owned`.
//! This module uses a custom visitor that accepts `visit_borrowed_str`
//! to produce `Cow::Borrowed` instead.
//!
//! Use with `#[serde(borrow, with = "crate::serde_cow_map")]`.

use std::borrow::Cow;

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeMap};

pub fn serialize<V: Serialize, S: Serializer>(
    map: &IndexMap<Cow<'_, str>, V>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut s = serializer.serialize_map(Some(map.len()))?;
    for (key, value) in map {
        s.serialize_entry(key.as_ref(), value)?;
    }
    s.end()
}

pub fn deserialize<'de: 'a, 'a, V, D>(
    deserializer: D,
) -> Result<IndexMap<Cow<'a, str>, V>, D::Error>
where
    V: Deserialize<'de>,
    D: Deserializer<'de>,
{
    deserializer.deserialize_map(MapVisitor(std::marker::PhantomData))
}

struct MapVisitor<V>(std::marker::PhantomData<V>);

impl<'de, V: Deserialize<'de>> de::Visitor<'de> for MapVisitor<V> {
    type Value = IndexMap<Cow<'de, str>, V>;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a map")
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let capacity = map.size_hint().unwrap_or(0).min(16384);
        let mut result = IndexMap::with_capacity(capacity);
        while let Some(key) = map.next_key_seed(BorrowedCowSeed)? {
            result.insert(key, map.next_value()?);
        }
        Ok(result)
    }
}

struct BorrowedCowSeed;

impl<'de> de::DeserializeSeed<'de> for BorrowedCowSeed {
    type Value = Cow<'de, str>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_str(BorrowedCowVisitor)
    }
}

struct BorrowedCowVisitor;

impl<'de> de::Visitor<'de> for BorrowedCowVisitor {
    type Value = Cow<'de, str>;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a string")
    }

    fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
        Ok(Cow::Borrowed(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(Cow::Owned(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(Cow::Owned(v))
    }
}
