use indexmap::IndexMap;
use std::{collections::HashMap, fmt::Debug};
use tracing::warn;

use crate::symbol::{Field, Symbol, Type};

#[derive(Debug, thiserror::Error, Clone)]
pub enum SymbolTypeTreeError {
    #[error("Couldn't get symbol type information: {0}")]
    MissingSymbolTypeInfo(String),
}

#[derive(Debug, PartialEq, Clone)]
pub enum SymbolTypeTree {
    Struct(IndexMap<String, (SymbolTypeTree, Option<u32>)>, usize),
    // Arrays not fully implemented.
    Array(Box<SymbolTypeTree>, usize),
    Void(usize),
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
    Unknown(usize),
    Missing,
}

impl SymbolTypeTree {
    pub fn get_size(&self) -> usize {
        match self {
            SymbolTypeTree::Struct(_index_map, size) => *size,
            SymbolTypeTree::Array(_symbol_type_tree, size) => *size,
            SymbolTypeTree::Void(size) => *size,
            SymbolTypeTree::Int => 2,
            SymbolTypeTree::Dint => 4,
            SymbolTypeTree::Real => 4,
            SymbolTypeTree::Lreal => 8,
            SymbolTypeTree::Sint => 1,
            SymbolTypeTree::Usint => 1,
            SymbolTypeTree::Uint => 2,
            SymbolTypeTree::Udint => 4,
            SymbolTypeTree::Lint => 8,
            SymbolTypeTree::Ulint => 8,
            SymbolTypeTree::String(size) => *size,
            SymbolTypeTree::Wstring(size) => *size,
            SymbolTypeTree::Real80 => 10,
            SymbolTypeTree::Bool => 1,
            SymbolTypeTree::Compound(size) => *size,
            SymbolTypeTree::Unknown(size) => *size,
            SymbolTypeTree::Missing => 0,
        }
    }
}

impl SymbolTypeTree {
    pub fn try_from_symbol(
        symbol: &Symbol,
        type_map: &HashMap<String, Type>,
    ) -> Result<Self, SymbolTypeTreeError> {
        Self::try_from_type_or_field(symbol.base_type, symbol.size, &symbol.typ, type_map)
    }

    pub fn try_from_field(
        field: &Field,
        type_map: &HashMap<String, Type>,
    ) -> Result<Self, SymbolTypeTreeError> {
        Self::try_from_type_or_field(field.base_type, field.size, &field.typ, type_map)
    }

    fn from_type_id(type_id: u32, size: usize) -> Self {
        match type_id {
            0 => Self::Void(size),
            2 => Self::Int,
            3 => Self::Dint,
            4 => Self::Real,
            5 => Self::Lreal,
            16 => Self::Sint,
            17 => Self::Usint,
            18 => Self::Uint,
            19 => Self::Udint,
            20 => Self::Lint,
            21 => Self::Ulint,
            30 => Self::String(size),
            31 => Self::Wstring(size),
            32 => Self::Real80,
            33 => Self::Bool,
            65 => Self::Compound(size),
            t => {
                warn!("Found unknown type number {t}.");
                Self::Unknown(size)
            }
        }
    }

    fn try_from_type_or_field(
        base_type: u32,
        size: usize,
        type_name: &str,
        type_map: &HashMap<String, Type>,
    ) -> Result<Self, SymbolTypeTreeError> {
        let mut tree = Self::from_type_id(base_type, size);

        if let SymbolTypeTree::Compound(_struct_size) = tree {
            let tree_type = type_map
                .get(type_name)
                .ok_or_else(|| SymbolTypeTreeError::MissingSymbolTypeInfo(type_name.to_string()))?;
            if !tree_type.fields.is_empty() {
                let mut struct_map = IndexMap::new();

                for field in &tree_type.fields {
                    struct_map.insert(
                        field.name.clone(),
                        (
                            Self::try_from_field(field, type_map)
                                .unwrap_or(SymbolTypeTree::Unknown(field.size)),
                            field.offset,
                        ),
                    );
                }

                tree = SymbolTypeTree::Struct(struct_map, tree_type.size);
            } else {
                tree = SymbolTypeTree::Array(Box::new(SymbolTypeTree::Usint), size);
                warn!("Dynamic symbol deserialisation is unsupported for arrays, converting to");
            }
        }

        Ok(tree)
    }

    // Symbol path nodes are separated by periods.
    pub fn get_child(&self, symbol_path: &str) -> &SymbolTypeTree {
        let path_tokens = symbol_path.split(".");
        let mut current = self;
        for token in path_tokens {
            if let Self::Struct(children, _size) = current {
                if let Some((child, _offset)) = children.get(token) {
                    current = child;
                    continue;
                }
            }
            return &Self::Missing;
        }
        current
    }
}
