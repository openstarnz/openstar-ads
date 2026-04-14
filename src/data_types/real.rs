use crate::data_types::PlcDataType;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct PlcReal(pub f32);

impl PlcDataType for PlcReal {}

impl From<f32> for PlcReal {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

impl From<PlcReal> for f32 {
    fn from(value: PlcReal) -> Self {
        value.0
    }
}
