use crate::data_types::PlcDataType;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct PlcUDInt(u32);

impl PlcDataType for PlcUDInt {}

impl From<[u16; 2]> for PlcUDInt {
    fn from(value: [u16; 2]) -> Self {
        Self(bytemuck::cast(value))
    }
}

impl From<u32> for PlcUDInt {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<PlcUDInt> for u32 {
    fn from(value: PlcUDInt) -> Self {
        value.0
    }
}
