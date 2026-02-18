use super::symbol_type_tree::SymbolTypeTree;
use extended::Extended;
use indexmap::IndexMap;
use zerocopy::FromBytes;

#[derive(Debug, Clone)]
pub enum SymbolTree {
    Missing,
    Malformed,
    Struct(IndexMap<String, SymbolTree>),
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
    Wstring(Extended),
    Bool(bool),
    Unknown,
}

impl From<(&SymbolTypeTree, &[u8], usize)> for SymbolTree {
    fn from((symbol_type_tree, data, parent_offset): (&SymbolTypeTree, &[u8], usize)) -> Self {
        let tree = match symbol_type_tree {
            SymbolTypeTree::Struct(tree_type_map, _) => {
                let mut tree_map = IndexMap::new();
                for ((name, offset), field_type_tree) in tree_type_map {
                    if let Some(offset) = offset {
                        let offset = *offset as usize;
                        tree_map.insert(
                            name.clone(),
                            (field_type_tree, data, parent_offset + offset).into(),
                        );
                    } else {
                        tree_map.insert(name.clone(), Self::Missing);
                    }
                }
                Self::Struct(tree_map)
            }
            SymbolTypeTree::Array(_symbol_type_tree, size) => {
                SymbolTree::Array([Self::Void(data.to_vec()[0..*size].to_vec())].to_vec())
            }
            SymbolTypeTree::Void(size) => Self::Void(data.to_vec()[0..*size].to_vec()),
            SymbolTypeTree::Int => match i16::read_from_prefix(&data) {
                Some(num) => Self::Int(num),
                None => Self::Malformed,
            },
            SymbolTypeTree::Dint => match i32::read_from_prefix(&data) {
                Some(num) => Self::Dint(num),
                None => Self::Malformed,
            },
            SymbolTypeTree::Real => match f32::read_from_prefix(&data) {
                Some(num) => Self::Real(num),
                None => Self::Malformed,
            },
            SymbolTypeTree::Lreal => match f64::read_from_prefix(&data) {
                Some(num) => Self::Lreal(num),
                None => Self::Malformed,
            },
            SymbolTypeTree::Sint => match i8::read_from_prefix(&data) {
                Some(num) => Self::Sint(num),
                None => Self::Malformed,
            },
            SymbolTypeTree::Usint => match u8::read_from_prefix(&data) {
                Some(num) => Self::Usint(num),
                None => Self::Malformed,
            },
            SymbolTypeTree::Uint => match u16::read_from_prefix(&data) {
                Some(num) => Self::Uint(num),
                None => Self::Malformed,
            },
            SymbolTypeTree::Udint => match u32::read_from_prefix(&data) {
                Some(num) => Self::Udint(num),
                None => Self::Malformed,
            },
            SymbolTypeTree::Lint => match i64::read_from_prefix(&data) {
                Some(num) => Self::Lint(num),
                None => Self::Malformed,
            },
            SymbolTypeTree::Ulint => match u64::read_from_prefix(&data) {
                Some(num) => Self::Ulint(num),
                None => Self::Malformed,
            },
            SymbolTypeTree::String(size) => {
                Self::String(String::from_utf8_lossy(&data.to_vec()[0..*size]).to_string())
            }
            SymbolTypeTree::Wstring(size) => {
                if size % 2 != 0 {
                    return Self::Malformed;
                }
                let mut words = Vec::new();
                for i in (0..*size).step_by(2) {
                    let Some(word) = u16::read_from(&data[i..i + 2]) else {
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
                buffer.copy_from_slice(&data[0..10]);

                // The beckhoff PLC system uses little endian byte order
                // https://infosys.beckhoff.com/english.php?content=../content/1033/tcplclib_tc2_utilities/35311883.html&id=
                Self::Lreal(extended::Extended::from_le_bytes(buffer).to_f64())
            }
            SymbolTypeTree::Bool => match u8::read_from(&data) {
                Some(num) => Self::Bool(num != 1),
                None => Self::Malformed,
            },
            SymbolTypeTree::Compound(_) => Self::Malformed,
            SymbolTypeTree::Unknown(_) => Self::Unknown,
            SymbolTypeTree::Missing => Self::Missing,
        };

        tree
    }
}
