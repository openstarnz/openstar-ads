use chrono::{DateTime, NaiveDate, Utc};
use indexmap::IndexMap;
use zerocopy::FromBytes;

use crate::data_types::symbol_type_tree::SymbolTypeTree;
pub type SymbolPath = String;
pub type SymbolOffset = u32;

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
    TimeStruct([usize; 7]),
}

pub struct PrimitiveSymbolDescriptor {
    symbol_type: PrimitiveSymbolType,
    offset: SymbolOffset,
}

impl From<(PrimitiveSymbolType, SymbolOffset)> for PrimitiveSymbolDescriptor {
    fn from((symbol_type, offset): (PrimitiveSymbolType, SymbolOffset)) -> Self {
        PrimitiveSymbolDescriptor {
            symbol_type,
            offset,
        }
    }
}

pub enum PrimitiveValue {
    Int(i64),
    Uint(u64),
    Float(f64),
    String(String),
    Timestamp(DateTime<Utc>),
    Bool(bool),
    Missing,
}

pub type SymbolMap = IndexMap<SymbolPath, PrimitiveValue>;
pub type SymbolTypeMap = IndexMap<SymbolPath, PrimitiveSymbolDescriptor>;

fn get_timestruct_keys() -> Vec<String> {
    vec![
        "year".into(),
        "month".into(),
        "day".into(),
        "hour".into(),
        "minute".into(),
        "second".into(),
        "milliseconds".into(),
    ]
}

pub trait SymbolTypeMapExt {
    fn from_tree(tree: SymbolTypeTree, offset: SymbolOffset, path: String) -> Self;
}

impl SymbolTypeMapExt for SymbolTypeMap {
    fn from_tree(tree: SymbolTypeTree, offset: SymbolOffset, path: String) -> SymbolTypeMap {
        let mut map = SymbolTypeMap::new();
        match tree {
            SymbolTypeTree::Struct(index_map, _size) => {
                let timestruct_keys = get_timestruct_keys();
                if index_map.keys().cloned().collect::<Vec<_>>() == timestruct_keys {
                    let mut offsets: [usize; 7] = [0; 7];
                    for (symbol_name, (symbol_type_tree, child_offset)) in index_map {
                        // Timestructs are invalid if a child offset is missing, a child type is not Uint, or if a child name is somehow not one of the correct keys
                        let Some(child_offset) = child_offset else {
                            return SymbolTypeMap::new();
                        };
                        let SymbolTypeTree::Uint = symbol_type_tree else {
                            return SymbolTypeMap::new();
                        };
                        let Some(index) = timestruct_keys.iter().position(|x| *x == symbol_name)
                        else {
                            return SymbolTypeMap::new();
                        };

                        // Retains the child offsets just in case they are not always the same relative offset from the struct.
                        offsets[index] = child_offset as usize;
                    }
                    map.insert(
                        path,
                        (PrimitiveSymbolType::TimeStruct(offsets), offset).into(),
                    );
                } else {
                    for (child_path, (child_tree, child_offset)) in index_map {
                        if let Some(child_offset) = child_offset {
                            map.append(&mut SymbolTypeMap::from_tree(
                                child_tree,
                                offset + child_offset,
                                format!("{path}.{child_path}"),
                            ));
                        }
                    }
                }
            }
            SymbolTypeTree::Int => {
                map.insert(path, (PrimitiveSymbolType::Int, offset).into());
            }
            SymbolTypeTree::Dint => {
                map.insert(path, (PrimitiveSymbolType::Dint, offset).into());
            }
            SymbolTypeTree::Real => {
                map.insert(path, (PrimitiveSymbolType::Real, offset).into());
            }
            SymbolTypeTree::Lreal => {
                map.insert(path, (PrimitiveSymbolType::Lreal, offset).into());
            }
            SymbolTypeTree::Sint => {
                map.insert(path, (PrimitiveSymbolType::Sint, offset).into());
            }
            SymbolTypeTree::Usint => {
                map.insert(path, (PrimitiveSymbolType::Usint, offset).into());
            }
            SymbolTypeTree::Uint => {
                map.insert(path, (PrimitiveSymbolType::Uint, offset).into());
            }
            SymbolTypeTree::Udint => {
                map.insert(path, (PrimitiveSymbolType::Udint, offset).into());
            }
            SymbolTypeTree::Lint => {
                map.insert(path, (PrimitiveSymbolType::Lint, offset).into());
            }
            SymbolTypeTree::Ulint => {
                map.insert(path, (PrimitiveSymbolType::Ulint, offset).into());
            }
            SymbolTypeTree::String(size) => {
                map.insert(path, (PrimitiveSymbolType::String(size), offset).into());
            }
            SymbolTypeTree::Wstring(size) => {
                map.insert(path, (PrimitiveSymbolType::Wstring(size), offset).into());
            }
            SymbolTypeTree::Real80 => {
                map.insert(path, (PrimitiveSymbolType::Real80, offset).into());
            }
            SymbolTypeTree::Bool => {
                map.insert(path, (PrimitiveSymbolType::Bool, offset).into());
            }
            _ => {} // SymbolTypeTree::Array(symbol_type_tree, _) => todo!(),
                    // SymbolTypeTree::Void(_) => todo!(),
                    // SymbolTypeTree::Compound(_) => todo!(),
                    // SymbolTypeTree::Unknown(_) => todo!(),
                    // SymbolTypeTree::Missing => todo!(),
        };

        map
    }
}

impl PrimitiveSymbolDescriptor {
    fn timestruct_from_bytes(offsets: [usize; 7], bytes: &[u8]) -> Option<DateTime<Utc>> {
        let year = u16::read_from_prefix(&bytes[offsets[0]..])?;
        let month = u16::read_from_prefix(&bytes[offsets[1]..])?;
        let day = u16::read_from_prefix(&bytes[offsets[2]..])?;
        let hour = u16::read_from_prefix(&bytes[offsets[3]..])?;
        let minute = u16::read_from_prefix(&bytes[offsets[4]..])?;
        let second = u16::read_from_prefix(&bytes[offsets[5]..])?;
        let milliseconds = u16::read_from_prefix(&bytes[offsets[6]..])?;

        NaiveDate::from_ymd_opt(year.into(), month.into(), day.into()).and_then(|date| {
            date.and_hms_milli_opt(
                hour.into(),
                minute.into(),
                second.into(),
                milliseconds.into(),
            )
            .map(|datetime| datetime.and_utc())
        })
    }

    pub fn read_from_bytes(&self, bytes: &[u8]) -> PrimitiveValue {
        let accessible_data: &[u8] = &bytes[(self.offset as usize)..];
        match self.symbol_type {
            PrimitiveSymbolType::TimeStruct(offsets) => {
                match Self::timestruct_from_bytes(offsets, accessible_data) {
                    Some(datetime) => PrimitiveValue::Timestamp(datetime),
                    None => PrimitiveValue::Missing,
                }
            }
            PrimitiveSymbolType::Real => match f32::read_from_prefix(accessible_data) {
                Some(float) => PrimitiveValue::Float(float as f64),
                None => PrimitiveValue::Missing,
            },
            PrimitiveSymbolType::Lreal => match f64::read_from_prefix(accessible_data) {
                Some(float) => PrimitiveValue::Float(float),
                None => PrimitiveValue::Missing,
            },
            PrimitiveSymbolType::Sint => match i8::read_from_prefix(accessible_data) {
                Some(integer) => PrimitiveValue::Int(integer as i64),
                None => PrimitiveValue::Missing,
            },
            PrimitiveSymbolType::Int => match i16::read_from_prefix(accessible_data) {
                Some(integer) => PrimitiveValue::Int(integer as i64),
                None => PrimitiveValue::Missing,
            },
            PrimitiveSymbolType::Dint => match i32::read_from_prefix(accessible_data) {
                Some(integer) => PrimitiveValue::Int(integer as i64),
                None => PrimitiveValue::Missing,
            },
            PrimitiveSymbolType::Lint => match i64::read_from_prefix(accessible_data) {
                Some(integer) => PrimitiveValue::Int(integer),
                None => PrimitiveValue::Missing,
            },
            PrimitiveSymbolType::Usint => match u8::read_from_prefix(accessible_data) {
                Some(uinteger) => PrimitiveValue::Uint(uinteger as u64),
                None => PrimitiveValue::Missing,
            },
            PrimitiveSymbolType::Uint => match u16::read_from_prefix(accessible_data) {
                Some(uinteger) => PrimitiveValue::Uint(uinteger as u64),
                None => PrimitiveValue::Missing,
            },
            PrimitiveSymbolType::Udint => match u32::read_from_prefix(accessible_data) {
                Some(uinteger) => PrimitiveValue::Uint(uinteger as u64),
                None => PrimitiveValue::Missing,
            },
            PrimitiveSymbolType::Ulint => match u64::read_from_prefix(accessible_data) {
                Some(uinteger) => PrimitiveValue::Uint(uinteger),
                None => PrimitiveValue::Missing,
            },
            PrimitiveSymbolType::String(size) => PrimitiveValue::String(
                String::from_utf8_lossy(&accessible_data.to_vec()[0..size]).to_string(),
            ),
            PrimitiveSymbolType::Bool => match u8::read_from(accessible_data) {
                Some(num) => PrimitiveValue::Bool(num != 1),
                None => PrimitiveValue::Missing,
            },
            PrimitiveSymbolType::Wstring(size) => {
                if size % 2 != 0 {
                    return PrimitiveValue::Missing;
                }
                let mut words = Vec::new();
                for i in (0..size).step_by(2) {
                    let Some(word) = u16::read_from(&accessible_data[i..i + 2]) else {
                        // If there is somehow not enough bytes in the data then the data is malformed.
                        return PrimitiveValue::Missing;
                    };
                    words.push(word);
                }

                PrimitiveValue::String(String::from_utf16_lossy(&words).to_string())
            }
            PrimitiveSymbolType::Real80 => {
                if bytes.len() < 10 {
                    return PrimitiveValue::Missing;
                }
                let mut buffer: [u8; 10] = [0; 10];
                buffer.copy_from_slice(&accessible_data[0..10]);

                // The beckhoff PLC system uses little endian byte order
                // https://infosys.beckhoff.com/english.php?content=../content/1033/tcplclib_tc2_utilities/35311883.html&id=
                // TODO: figure out a good way to deal with these
                PrimitiveValue::Float(extended::Extended::from_le_bytes(buffer).to_f64())
            }
        }
    }
}

pub trait SymbolMapExt {
    fn from_bytes(symbol_type_map: &SymbolTypeMap, bytes: &[u8]) -> Self;
}

impl SymbolMapExt for SymbolMap {
    fn from_bytes(symbol_type_map: &SymbolTypeMap, bytes: &[u8]) -> SymbolMap {
        let mut map = SymbolMap::new();
        for (path, symbol_descriptor) in symbol_type_map {
            map.insert(path.to_string(), symbol_descriptor.read_from_bytes(bytes));
        }
        map
    }
}
