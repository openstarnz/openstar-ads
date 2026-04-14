use crate::data_types::PlcDataType;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct PlcLReal(f64);

impl PlcDataType for PlcLReal {}

impl From<f64> for PlcLReal {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

impl From<PlcLReal> for f64 {
    fn from(value: PlcLReal) -> Self {
        value.0
    }
}
