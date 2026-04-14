use crate::data_types::PlcDataType;

use super::udint::PlcUDInt;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct PlcTime(PlcUDInt);

impl PlcDataType for PlcTime {}

impl From<u32> for PlcTime {
    fn from(value: u32) -> Self {
        Self(PlcUDInt::from(value))
    }
}

impl From<PlcTime> for u32 {
    fn from(value: PlcTime) -> Self {
        value.0.into()
    }
}
