pub mod primitives;
pub mod symbol_map;
pub mod symbol_tree;
pub mod symbol_type_tree;

use std::fmt::Debug;

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
