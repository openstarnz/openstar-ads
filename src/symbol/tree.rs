use std::fmt::Display;

use super::type_tree::SymbolTypeTree;
use chrono::{DateTime, NaiveDate, Utc};
use indexmap::IndexMap;
use serde::{
    de::{MapAccess, SeqAccess},
    forward_to_deserialize_any, Deserializer, Serialize,
};
use thiserror::Error;
use tracing::warn;
use zerocopy::FromBytes;

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
/// A tree representation of ADS symbol data. Has the ability to be deserialised indirectly into types with serde
pub enum SymbolTree {
    Missing,
    Malformed,
    Struct(IndexMap<String, SymbolTree>),
    /// Not fully implemented.
    Array(Vec<SymbolTree>),
    Void(Vec<u8>),
    Int(i16),
    Dint(i32),
    Real(f32),
    Lreal(f64),
    Sint(i8),
    Usint(u8),
    Uint(u16),
    Udint(u32),
    Lint(i64),
    Ulint(u64),
    String(String),
    Real80(f64),
    Bool(bool),
    Unknown,
}

impl SymbolTree {
    pub fn from_bytes(
        data: &[u8],
        symbol_type_tree: &SymbolTypeTree,
        parent_offset: usize,
    ) -> Self {
        if parent_offset > data.len() {
            warn!(
                "Offset of {parent_offset} greater than data length of {}.",
                data.len()
            );
            return Self::Missing;
        }
        let accessible_data = &data[parent_offset..];
        let tree = match symbol_type_tree {
            SymbolTypeTree::Struct(tree_type_map, _) => {
                let mut tree_map = IndexMap::new();
                for (name, (field_type_tree, offset)) in tree_type_map {
                    if let Some(offset) = offset {
                        let child_offset = *offset as usize;
                        tree_map.insert(
                            name.clone(),
                            SymbolTree::from_bytes(
                                data,
                                field_type_tree,
                                parent_offset + child_offset,
                            ),
                        );
                    } else {
                        tree_map.insert(name.clone(), Self::Missing);
                    }
                }
                Self::Struct(tree_map)
            }
            // Arrays are not implemented fully but this will at least provide the raw data.
            SymbolTypeTree::Array(_symbol_type_tree, size) => {
                SymbolTree::Array([Self::Void(accessible_data[0..*size].to_vec())].to_vec())
            }
            SymbolTypeTree::Void(size) => Self::Void(accessible_data[0..*size].to_vec()),
            SymbolTypeTree::Int => match i16::read_from_prefix(accessible_data) {
                Ok((num, _)) => Self::Int(num),
                Err(_error) => Self::Malformed,
            },
            SymbolTypeTree::Dint => match i32::read_from_prefix(accessible_data) {
                Ok((num, _)) => Self::Dint(num),
                Err(_error) => Self::Malformed,
            },
            SymbolTypeTree::Real => match f32::read_from_prefix(accessible_data) {
                Ok((num, _)) => Self::Real(num),
                Err(_error) => Self::Malformed,
            },
            SymbolTypeTree::Lreal => match f64::read_from_prefix(accessible_data) {
                Ok((num, _)) => Self::Lreal(num),
                Err(_error) => Self::Malformed,
            },
            SymbolTypeTree::Sint => match i8::read_from_prefix(accessible_data) {
                Ok((num, _)) => Self::Sint(num),
                Err(_error) => Self::Malformed,
            },
            SymbolTypeTree::Usint => match u8::read_from_prefix(accessible_data) {
                Ok((num, _)) => Self::Usint(num),
                Err(_error) => Self::Malformed,
            },
            SymbolTypeTree::Uint => match u16::read_from_prefix(accessible_data) {
                Ok((num, _)) => Self::Uint(num),
                Err(_error) => Self::Malformed,
            },
            SymbolTypeTree::Udint => match u32::read_from_prefix(accessible_data) {
                Ok((num, _)) => Self::Udint(num),
                Err(_error) => Self::Malformed,
            },
            SymbolTypeTree::Lint => match i64::read_from_prefix(accessible_data) {
                Ok((num, _)) => Self::Lint(num),
                Err(_error) => Self::Malformed,
            },
            SymbolTypeTree::Ulint => match u64::read_from_prefix(accessible_data) {
                Ok((num, _)) => Self::Ulint(num),
                Err(_error) => Self::Malformed,
            },
            SymbolTypeTree::String(size) => Self::String(
                String::from_utf8_lossy(&accessible_data.to_vec()[0..*size]).to_string(),
            ),
            SymbolTypeTree::Wstring(size) => {
                if size % 2 != 0 {
                    return Self::Malformed;
                }
                let mut words = Vec::new();
                for i in (0..*size).step_by(2) {
                    let Ok(word) = u16::read_from_bytes(&accessible_data[i..i + 2]) else {
                        // If there is somehow not enough bytes in the data then the data is malformed.
                        return Self::Malformed;
                    };
                    words.push(word);
                }

                Self::String(String::from_utf16_lossy(&words).to_string())
            }
            SymbolTypeTree::Real80 => {
                if accessible_data.len() < 10 {
                    return Self::Malformed;
                }
                let mut buffer: [u8; 10] = [0; 10];
                buffer.copy_from_slice(&accessible_data[0..10]);

                // The beckhoff PLC system uses little endian byte order
                // https://infosys.beckhoff.com/english.php?content=../content/1033/tcplclib_tc2_utilities/35311883.html&id=
                // TODO: Use actual extended value and figure out serde for that.
                Self::Real80(extended::Extended::from_le_bytes(buffer).to_f64())
            }
            SymbolTypeTree::Bool => match u8::read_from_prefix(accessible_data) {
                Ok((num, _)) => Self::Bool(num != 0),
                Err(_error) => Self::Malformed,
            },
            SymbolTypeTree::Compound(_) => Self::Malformed,
            SymbolTypeTree::Unknown(_) => Self::Unknown,
            SymbolTypeTree::Missing => Self::Missing,
        };

        tree
    }
}

impl SymbolTree {
    // Symbol path nodes are separated by periods.
    pub fn get_child(&self, symbol_path: &str) -> &SymbolTree {
        let path_tokens = symbol_path.split(".");
        let mut current = self;
        for token in path_tokens {
            if let Self::Struct(children) = current {
                if let Some(child) = children.get(token) {
                    current = child;
                    continue;
                }
            }
            return &Self::Missing;
        }
        current
    }

    pub fn get_timestamp(&self, path: &str) -> Option<DateTime<Utc>> {
        if let SymbolTree::Struct(map) = self.get_child(path) {
            let SymbolTree::Uint(year) = *map.get("wYear")? else {
                return None;
            };
            let SymbolTree::Uint(month) = *map.get("wMonth")? else {
                return None;
            };
            let SymbolTree::Uint(day) = *map.get("wDay")? else {
                return None;
            };
            let SymbolTree::Uint(hour) = *map.get("wHour")? else {
                return None;
            };
            let SymbolTree::Uint(minute) = *map.get("wMinute")? else {
                return None;
            };
            let SymbolTree::Uint(second) = *map.get("wSecond")? else {
                return None;
            };
            let SymbolTree::Uint(milliseconds) = *map.get("wMilliseconds")? else {
                return None;
            };

            NaiveDate::from_ymd_opt(year.into(), month.into(), day.into()).and_then(|date| {
                date.and_hms_milli_opt(
                    hour.into(),
                    minute.into(),
                    second.into(),
                    milliseconds.into(),
                )
                .map(|datetime| datetime.and_utc())
            })
        } else {
            None
        }
    }
}

impl<'de> Deserializer<'de> for &SymbolTree {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            SymbolTree::Struct(index_map) => visitor.visit_map(MapAccessor::new(index_map)),
            SymbolTree::Array(symbol_trees) => visitor.visit_seq(SeqAccessor::new(symbol_trees)),
            SymbolTree::Int(v) => visitor.visit_i16(*v),
            SymbolTree::Dint(v) => visitor.visit_i32(*v),
            SymbolTree::Real(v) => visitor.visit_f32(*v),
            SymbolTree::Lreal(v) => visitor.visit_f64(*v),
            SymbolTree::Sint(v) => visitor.visit_i8(*v),
            SymbolTree::Usint(v) => visitor.visit_u8(*v),
            SymbolTree::Uint(v) => visitor.visit_u16(*v),
            SymbolTree::Udint(v) => visitor.visit_u32(*v),
            SymbolTree::Lint(v) => visitor.visit_i64(*v),
            SymbolTree::Ulint(v) => visitor.visit_u64(*v),
            SymbolTree::String(v) => visitor.visit_string(v.clone()),
            SymbolTree::Real80(v) => visitor.visit_f64(*v),
            SymbolTree::Bool(v) => visitor.visit_bool(*v),
            SymbolTree::Missing => visitor.visit_none(),
            SymbolTree::Unknown => Err(Error::Deserialisation(
                "unable to deserialize SymbolTree::Unknown".to_owned(),
            )),
            SymbolTree::Void(bytes) => visitor.visit_bytes(bytes),
            SymbolTree::Malformed => Err(Error::Deserialisation(
                "unable to deserialize SymbolTree::Malformed".to_owned(),
            )),
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            SymbolTree::Missing => visitor.visit_none(),
            t => visitor.visit_some(t),
        }
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Couldn't deserialise symbol tree with message: {0}")]
    Deserialisation(String),
}

impl serde::de::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::Deserialisation(msg.to_string())
    }
}

struct MapAccessor<'a> {
    items: &'a IndexMap<String, SymbolTree>,
    key_index: usize,
}

impl<'a> MapAccessor<'a> {
    pub fn new(items: &'a IndexMap<String, SymbolTree>) -> Self {
        Self {
            items,
            key_index: 0,
        }
    }
}

impl<'a, 'de: 'a> MapAccess<'de> for MapAccessor<'a> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        self.items
            .get_index(self.key_index)
            .map(|(key, _)| seed.deserialize(KeyDeserializer(key)))
            .transpose()
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        if let Some((_, value)) = self.items.get_index(self.key_index) {
            // Increment the key index only after the value has been retrieved
            self.key_index += 1;
            return seed.deserialize(value);
        }
        Err(Error::Deserialisation(
            "failed to get next value from map".to_owned(),
        ))
    }
}

struct SeqAccessor<'a> {
    items: &'a [SymbolTree],
    index: usize,
}

impl<'a> SeqAccessor<'a> {
    fn new(items: &'a [SymbolTree]) -> Self {
        Self { items, index: 0 }
    }
}

impl<'a, 'de: 'a> SeqAccess<'de> for SeqAccessor<'a> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        self.items
            .get(self.index)
            .map(|item| {
                self.index += 1;
                seed.deserialize(item)
            })
            .transpose()
    }
}

struct KeyDeserializer<'a>(&'a str);

impl<'de, 'a> Deserializer<'de> for KeyDeserializer<'a> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_str(self.0)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}
