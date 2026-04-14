use crate::data_types::PlcDataType;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct PlcDInt(i32);

impl PlcDataType for PlcDInt {}

impl From<PlcDInt> for i32 {
    fn from(value: PlcDInt) -> Self {
        value.0
    }
}
