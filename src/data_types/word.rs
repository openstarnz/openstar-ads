use crate::data_types::PlcDataType;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct PlcWord(u16);

impl PlcDataType for PlcWord {}

impl From<PlcWord> for u16 {
    fn from(value: PlcWord) -> Self {
        value.0
    }
}
