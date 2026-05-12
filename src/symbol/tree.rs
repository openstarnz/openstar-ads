use super::type_tree::SymbolTypeTree;
use bytes::Bytes;
use chrono::{DateTime, NaiveDate, Utc};
use indexmap::IndexMap;
use serde::Serialize;
use tracing::warn;
use zerocopy::FromBytes;

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum SymbolTree {
    Missing,
    Malformed,
    Struct(IndexMap<String, SymbolTree>),
    // Not fully implemented.
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
        data: Bytes,
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
                                data.clone(),
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
                if data.len() < 10 {
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
            let SymbolTree::Uint(year) = *map.get("year")? else {
                return None;
            };
            let SymbolTree::Uint(month) = *map.get("month")? else {
                return None;
            };
            let SymbolTree::Uint(day) = *map.get("day")? else {
                return None;
            };
            let SymbolTree::Uint(hour) = *map.get("hour")? else {
                return None;
            };
            let SymbolTree::Uint(minute) = *map.get("minute")? else {
                return None;
            };
            let SymbolTree::Uint(second) = *map.get("second")? else {
                return None;
            };
            let SymbolTree::Uint(milliseconds) = *map.get("milliseconds")? else {
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
