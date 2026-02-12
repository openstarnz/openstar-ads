pub mod primitives;

use ads::symbol::{Field, Symbol, Type};
use anyhow::Context;
use indexmap::IndexMap;
use std::{collections::HashMap, fmt::Debug};
use zerocopy::FromBytes;

pub trait PlcDataType:
    Clone + Debug + Default + zerocopy::AsBytes + zerocopy::FromBytes + zerocopy::FromZeroes
{
    fn size() -> usize {
        std::mem::size_of::<Self>()
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Self::read_from(bytes)
    }
}

#[derive(Debug, PartialEq)]
pub enum SymbolTypeTree {
    Struct(IndexMap<(String, Option<u32>), SymbolTypeTree>),
    Array(Box<SymbolTypeTree>, usize),
    Void,
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
    Compound(usize),
    Unknown,
}

impl From<(u32, usize)> for SymbolTypeTree {
    fn from((type_id, size): (u32, usize)) -> Self {
        match type_id {
            0 => Self::Void,
            2 => Self::Int,
            3 => Self::Dint,
            4 => Self::Real,
            5 => Self::Lreal,
            16 => Self::Sint,
            17 => Self::Usint,
            18 => Self::Uint,
            19 => Self::Udint,
            20 => Self::Lint,
            30 => Self::String(size),
            31 => Self::Wstring(size),
            32 => Self::Real80,
            33 => Self::Bool,
            65 => Self::Compound(size),
            _ => Self::Unknown,
        }
    }
}

impl TryFrom<(&Symbol, &HashMap<String, Type>)> for SymbolTypeTree {
    type Error = anyhow::Error;

    fn try_from(
        (symbol, type_map): (&Symbol, &HashMap<String, Type>),
    ) -> Result<Self, Self::Error> {
        let mut tree = (symbol.base_type, symbol.size).into();

        if let Self::Compound(_struct_size) = tree {
            let symbol_type = type_map
                .get(&symbol.name)
                .context("Couldn't get symbol type information.")?;
            if symbol_type.fields.len() > 0 {
                let mut struct_map = IndexMap::new();

                for field in &symbol_type.fields {
                    struct_map.insert(
                        (field.name.clone(), field.offset),
                        (field, type_map).try_into().unwrap_or(Self::Unknown),
                    );
                }

                tree = Self::Struct(struct_map);
            } else {
                println!("Dynamic symbol deserialisation is unsupported for arrays");
            }
        }

        Ok(tree)
    }
}

impl TryFrom<(&Field, &HashMap<String, Type>)> for SymbolTypeTree {
    type Error = anyhow::Error;

    fn try_from((field, type_map): (&Field, &HashMap<String, Type>)) -> Result<Self, Self::Error> {
        let mut tree = (field.base_type, field.size).into();

        if let Self::Compound(_struct_size) = tree {
            let tree_type = type_map
                .get(&field.name)
                .context("Couldn't get symbol type information.")?;
            if tree_type.fields.len() > 0 {
                let mut struct_map = IndexMap::new();

                for field in &tree_type.fields {
                    struct_map.insert(
                        (field.name.clone(), field.offset),
                        (field, type_map).try_into().unwrap_or(Self::Unknown),
                    );
                }

                tree = Self::Struct(struct_map);
            } else {
                println!("Dynamic symbol deserialisation is unsupported for arrays");
            }
        }

        Ok(tree)
    }
}

pub enum SymbolTree {
    Missing,
    Malformed,
    Struct(IndexMap<String, SymbolTree>),
    Array(Vec<SymbolTree>),
    Void,
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
    Wstring(Vec<u16>),
    Real80([u8; 10]),
    Bool(bool),
    Unknown,
}

impl From<(&SymbolTypeTree, &[u8], usize)> for SymbolTree {
    fn from((symbol_type_tree, data, parent_offset): (&SymbolTypeTree, &[u8], usize)) -> Self {
        let tree = match symbol_type_tree {
            SymbolTypeTree::Struct(tree_type_map) => {
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
            SymbolTypeTree::Array(_symbol_type_tree, _size) => Self::Void,
            SymbolTypeTree::Void => Self::Void,
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
            SymbolTypeTree::String(_) => Self::Missing,
            SymbolTypeTree::Wstring(_) => Self::Missing,
            SymbolTypeTree::Real80 => Self::Malformed,
            SymbolTypeTree::Bool => match u8::read_from(&data) {
                Some(num) => Self::Bool(num != 1),
                None => Self::Malformed,
            },
            SymbolTypeTree::Compound(_) => Self::Malformed,
            SymbolTypeTree::Unknown => Self::Unknown,
        };

        tree
    }
}
