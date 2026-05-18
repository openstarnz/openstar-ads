use std::fmt::Display;

use bytes::Bytes;
use chrono::{DateTime, NaiveDate, Utc};
use indexmap::IndexMap;
use serde::Serialize;
use zerocopy::{FromBytes, SizeError};

use crate::SymbolTypeTree;

pub type SymbolPath = String;
pub type SymbolOffset = u32;
pub type SymbolMap = IndexMap<SymbolPath, PrimitiveValue>;
pub type SymbolTypeMap = IndexMap<SymbolPath, PrimitiveSymbolDescriptor>;

pub const TIMESTRUCT_KEYS: [&str; 8] = [
    "wYear",
    "wMonth",
    "wDayOfWeek",
    "wDay",
    "wHour",
    "wMinute",
    "wSecond",
    "wMilliseconds",
];

pub enum PrimitiveSymbolType {
    Int,
    Dint,
    Real,
    Lreal,
    Sint,
    Usint,
    Uint,
    Udint,
    Lint,
    Ulint,
    String(usize),
    Wstring(usize),
    Real80,
    Bool,
    TimeStruct([usize; 8]),
    Void(usize),
}

pub struct PrimitiveSymbolDescriptor {
    symbol_type: PrimitiveSymbolType,
    root_symbol_offset: SymbolOffset,
}

impl PrimitiveSymbolDescriptor {
    pub fn from_type(symbol_type: PrimitiveSymbolType, offset: SymbolOffset) -> Self {
        PrimitiveSymbolDescriptor {
            symbol_type,
            root_symbol_offset: offset,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize)]
#[serde(untagged)]
pub enum PrimitiveValue {
    Int(i64),
    Uint(u64),
    Float(f64),
    String(String),
    Timestamp(DateTime<Utc>),
    Bool(bool),
    Malformed,
    Void(Vec<u8>),
}

impl Display for PrimitiveValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrimitiveValue::Int(value) => f.write_str(&value.to_string()),
            PrimitiveValue::Uint(value) => f.write_str(&value.to_string()),
            PrimitiveValue::Float(value) => f.write_str(&value.to_string()),
            PrimitiveValue::String(value) => f.write_str(&value.to_string()),
            PrimitiveValue::Timestamp(value) => f.write_str(&value.to_string()),
            PrimitiveValue::Bool(value) => f.write_str(&value.to_string()),
            PrimitiveValue::Malformed => f.write_str("Malformed"),
            PrimitiveValue::Void(_items) => f.write_str("Void"),
        }
    }
}

pub trait SymbolTypeMapExt {
    fn from_tree(tree: SymbolTypeTree, offset: SymbolOffset, path: String) -> Self;
}

impl SymbolTypeMapExt for SymbolTypeMap {
    fn from_tree(tree: SymbolTypeTree, offset: SymbolOffset, path: String) -> SymbolTypeMap {
        let mut map = SymbolTypeMap::new();
        match tree {
            SymbolTypeTree::Struct(index_map, _size) => {
                if index_map.keys().collect::<Vec<_>>() == TIMESTRUCT_KEYS {
                    let mut offsets: [usize; 8] = [0; 8];
                    for (symbol_name, (symbol_type_tree, child_offset)) in index_map {
                        // Timestructs are invalid if a child offset is missing ...
                        let Some(child_offset) = child_offset else {
                            return SymbolTypeMap::new();
                        };

                        // or a child type is not Uint ...
                        let SymbolTypeTree::Uint = symbol_type_tree else {
                            return SymbolTypeMap::new();
                        };

                        // or if a child name is somehow not one of the correct keys ...
                        let Some(index) = TIMESTRUCT_KEYS.iter().position(|x| *x == symbol_name)
                        else {
                            return SymbolTypeMap::new();
                        };

                        // Retains the child offsets just in case they are not always the same relative offset from the struct.
                        offsets[index] = child_offset as usize;
                    }
                    map.insert(
                        path,
                        PrimitiveSymbolDescriptor::from_type(
                            PrimitiveSymbolType::TimeStruct(offsets),
                            offset,
                        ),
                    );
                } else {
                    for (child_path, (child_tree, child_offset)) in index_map {
                        if let Some(child_offset) = child_offset {
                            if path.is_empty() {
                                map.append(&mut SymbolTypeMap::from_tree(
                                    child_tree,
                                    offset + child_offset,
                                    child_path.to_string(),
                                ));
                            } else {
                                map.append(&mut SymbolTypeMap::from_tree(
                                    child_tree,
                                    offset + child_offset,
                                    format!("{path}.{child_path}"),
                                ));
                            }
                        }
                    }
                }
            }
            SymbolTypeTree::Int => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Int, offset),
                );
            }
            SymbolTypeTree::Dint => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Dint, offset),
                );
            }
            SymbolTypeTree::Real => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Real, offset),
                );
            }
            SymbolTypeTree::Lreal => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Lreal, offset),
                );
            }
            SymbolTypeTree::Sint => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Sint, offset),
                );
            }
            SymbolTypeTree::Usint => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Usint, offset),
                );
            }
            SymbolTypeTree::Uint => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Uint, offset),
                );
            }
            SymbolTypeTree::Udint => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Udint, offset),
                );
            }
            SymbolTypeTree::Lint => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Lint, offset),
                );
            }
            SymbolTypeTree::Ulint => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Ulint, offset),
                );
            }
            SymbolTypeTree::String(size) => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::String(size), offset),
                );
            }
            SymbolTypeTree::Wstring(size) => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(
                        PrimitiveSymbolType::Wstring(size),
                        offset,
                    ),
                );
            }
            SymbolTypeTree::Real80 => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Real80, offset),
                );
            }
            SymbolTypeTree::Bool => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Bool, offset),
                );
            }
            // This is not implemented yet as getting the type the Array contains is elusive
            // Converts to void so that the consumer can attempt to retrieve the data
            SymbolTypeTree::Array(_symbol_type_tree, size) => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Void(size), offset),
                );
            }
            SymbolTypeTree::Void(size) => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Void(size), offset),
                );
            }
            // Converts to void so that the consumer can attempt to retrieve the data
            SymbolTypeTree::Unknown(size) => {
                map.insert(
                    path,
                    PrimitiveSymbolDescriptor::from_type(PrimitiveSymbolType::Void(size), offset),
                );
            }
            // This is a conversion type and represents that
            SymbolTypeTree::Compound(_) => {
                unimplemented!("Conversion of unspecified Compount type not possible.");
            }
            // Missing means not in the structure so we don't add it to the map
            SymbolTypeTree::Missing => {}
        };

        map
    }
}

impl PrimitiveSymbolDescriptor {
    fn datetime_from_bytes(
        offsets: [usize; 8],
        bytes: &[u8],
    ) -> Result<Option<DateTime<Utc>>, SizeError<&[u8], u16>> {
        let (year, _) = u16::read_from_prefix(&bytes[offsets[0]..])?;
        let (month, _) = u16::read_from_prefix(&bytes[offsets[1]..])?;
        let (day, _) = u16::read_from_prefix(&bytes[offsets[3]..])?;
        let (hour, _) = u16::read_from_prefix(&bytes[offsets[4]..])?;
        let (minute, _) = u16::read_from_prefix(&bytes[offsets[5]..])?;
        let (second, _) = u16::read_from_prefix(&bytes[offsets[6]..])?;
        let (milliseconds, _) = u16::read_from_prefix(&bytes[offsets[7]..])?;

        Ok(
            NaiveDate::from_ymd_opt(year.into(), month.into(), day.into()).and_then(|date| {
                date.and_hms_milli_opt(
                    hour.into(),
                    minute.into(),
                    second.into(),
                    milliseconds.into(),
                )
                .map(|datetime| datetime.and_utc())
            }),
        )
    }

    pub fn read_from_bytes(&self, bytes: &[u8]) -> PrimitiveValue {
        let accessible_data: &[u8] = &bytes[(self.root_symbol_offset as usize)..];
        match self.symbol_type {
            PrimitiveSymbolType::TimeStruct(offsets) => {
                match Self::datetime_from_bytes(offsets, accessible_data) {
                    Ok(Some(datetime)) => PrimitiveValue::Timestamp(datetime),
                    Err(_) | Ok(None) => PrimitiveValue::Malformed,
                }
            }
            PrimitiveSymbolType::Real => match f32::read_from_prefix(accessible_data) {
                Ok((float, _)) => PrimitiveValue::Float(float as f64),
                Err(_error) => PrimitiveValue::Malformed,
            },
            PrimitiveSymbolType::Lreal => match f64::read_from_prefix(accessible_data) {
                Ok((float, _)) => PrimitiveValue::Float(float),
                Err(_error) => PrimitiveValue::Malformed,
            },
            PrimitiveSymbolType::Sint => match i8::read_from_prefix(accessible_data) {
                Ok((integer, _)) => PrimitiveValue::Int(integer as i64),
                Err(_error) => PrimitiveValue::Malformed,
            },
            PrimitiveSymbolType::Int => match i16::read_from_prefix(accessible_data) {
                Ok((integer, _)) => PrimitiveValue::Int(integer as i64),
                Err(_error) => PrimitiveValue::Malformed,
            },
            PrimitiveSymbolType::Dint => match i32::read_from_prefix(accessible_data) {
                Ok((integer, _)) => PrimitiveValue::Int(integer as i64),
                Err(_error) => PrimitiveValue::Malformed,
            },
            PrimitiveSymbolType::Lint => match i64::read_from_prefix(accessible_data) {
                Ok((integer, _)) => PrimitiveValue::Int(integer),
                Err(_error) => PrimitiveValue::Malformed,
            },
            PrimitiveSymbolType::Usint => match u8::read_from_prefix(accessible_data) {
                Ok((uinteger, _)) => PrimitiveValue::Uint(uinteger as u64),
                Err(_error) => PrimitiveValue::Malformed,
            },
            PrimitiveSymbolType::Uint => match u16::read_from_prefix(accessible_data) {
                Ok((uinteger, _)) => PrimitiveValue::Uint(uinteger as u64),
                Err(_error) => PrimitiveValue::Malformed,
            },
            PrimitiveSymbolType::Udint => match u32::read_from_prefix(accessible_data) {
                Ok((uinteger, _)) => PrimitiveValue::Uint(uinteger as u64),
                Err(_error) => PrimitiveValue::Malformed,
            },
            PrimitiveSymbolType::Ulint => match u64::read_from_prefix(accessible_data) {
                Ok((uinteger, _)) => PrimitiveValue::Uint(uinteger),
                Err(_error) => PrimitiveValue::Malformed,
            },
            PrimitiveSymbolType::String(size) => PrimitiveValue::String(
                String::from_utf8_lossy(&accessible_data.to_vec()[0..size]).to_string(),
            ),
            PrimitiveSymbolType::Bool => match u8::read_from_prefix(accessible_data) {
                Ok((num, _)) => PrimitiveValue::Bool(num != 0),
                Err(_error) => PrimitiveValue::Malformed,
            },
            PrimitiveSymbolType::Wstring(size) => {
                if size % 2 != 0 {
                    return PrimitiveValue::Malformed;
                }
                let mut words = Vec::new();
                for i in (0..size).step_by(2) {
                    let Ok(word) = u16::read_from_bytes(&accessible_data[i..i + 2]) else {
                        // If there is somehow not enough bytes in the data then the data is malformed.
                        return PrimitiveValue::Malformed;
                    };
                    words.push(word);
                }

                PrimitiveValue::String(String::from_utf16_lossy(&words).to_string())
            }
            PrimitiveSymbolType::Real80 => {
                if bytes.len() < 10 {
                    return PrimitiveValue::Malformed;
                }
                let mut buffer: [u8; 10] = [0; 10];
                buffer.copy_from_slice(&accessible_data[0..10]);

                // The beckhoff PLC system uses little endian byte order
                // https://infosys.beckhoff.com/english.php?content=../content/1033/tcplclib_tc2_utilities/35311883.html&id=
                // TODO: figure out a good way to deal with these
                PrimitiveValue::Float(extended::Extended::from_le_bytes(buffer).to_f64())
            }
            PrimitiveSymbolType::Void(size) => {
                PrimitiveValue::Void(accessible_data[0..size].to_vec())
            }
        }
    }
}

pub trait SymbolMapExt {
    fn from_bytes(bytes: Bytes, symbol_type_map: &SymbolTypeMap) -> Self;
}

impl SymbolMapExt for SymbolMap {
    fn from_bytes(bytes: Bytes, symbol_type_map: &SymbolTypeMap) -> SymbolMap {
        let mut map = SymbolMap::new();
        for (path, symbol_descriptor) in symbol_type_map {
            map.insert(path.to_string(), symbol_descriptor.read_from_bytes(&bytes));
        }
        map
    }
}
