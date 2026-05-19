pub mod bool;
pub mod dint;
pub mod int;
pub mod lreal;
pub mod real;
pub mod string16;
pub mod time;
pub mod time_struct;
pub mod udint;
pub mod uint;
pub mod word;

use std::fmt::Debug;

pub trait AdsData:
    Clone
    + Debug
    + Default
    + zerocopy::Immutable
    + zerocopy::IntoBytes
    + zerocopy::FromBytes
    + zerocopy::FromZeros
{
    fn size() -> usize {
        std::mem::size_of::<Self>()
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Self::read_from_bytes(bytes).ok()
    }
}
