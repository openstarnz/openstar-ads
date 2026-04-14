use crate::data_types::PlcDataType;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct PlcUInt(u16);

impl PlcDataType for PlcUInt {}

impl From<u16> for PlcUInt {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<PlcUInt> for u16 {
    fn from(value: PlcUInt) -> Self {
        value.0
    }
}
