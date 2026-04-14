use crate::data_types::PlcDataType;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct PlcInt(i16);

impl PlcDataType for PlcInt {}

impl From<i16> for PlcInt {
    fn from(value: i16) -> Self {
        Self(value)
    }
}

impl From<PlcInt> for i16 {
    fn from(value: PlcInt) -> Self {
        value.0
    }
}
