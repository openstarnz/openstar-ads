use indexmap::IndexMap;

use crate::data_types::symbol_type_tree::SymbolTypeTree;

pub type Nanoseconds = u64;
pub type SymbolPath = String;
pub type SymbolOffset = u64;

pub enum PrimitiveType {
    Int(i64),
    Uint(u64),
    Float(f64),
    String(String),
    Timestamp(Nanoseconds),
}

pub type SymbolMap = IndexMap<SymbolPath, PrimitiveType>;
pub type SymbolTypeMap = IndexMap<SymbolPath, usize>;

impl From<(SymbolTypeTree, SymbolOffset)> for SymbolTypeMap {
    fn into((tree, offset): (SymbolTypeTree, SymbolOffset)) -> SymbolTypeMap {
        match self {
            SymbolTypeTree::Struct(index_map, offset) => todo!(),
            SymbolTypeTree::Int => todo!(),
            SymbolTypeTree::Dint => todo!(),
            SymbolTypeTree::Real => todo!(),
            SymbolTypeTree::Lreal => todo!(),
            SymbolTypeTree::Sint => todo!(),
            SymbolTypeTree::Usint => todo!(),
            SymbolTypeTree::Uint => todo!(),
            SymbolTypeTree::Udint => todo!(),
            SymbolTypeTree::Lint => todo!(),
            SymbolTypeTree::Ulint => todo!(),
            SymbolTypeTree::String(_) => todo!(),
            SymbolTypeTree::Wstring(_) => todo!(),
            SymbolTypeTree::Real80 => todo!(),
            SymbolTypeTree::Bool => todo!(),
            _ => SymbolTypeMap::new(), // SymbolTypeTree::Array(symbol_type_tree, _) => todo!(),
                                       // SymbolTypeTree::Void(_) => todo!(),
                                       // SymbolTypeTree::Compound(_) => todo!(),
                                       // SymbolTypeTree::Unknown(_) => todo!(),
                                       // SymbolTypeTree::Missing => todo!(),
        }
    }
}
