use crate::data_types::PlcDataType;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct PlcBool(u8);

impl PlcDataType for PlcBool {}

impl From<bool> for PlcBool {
    fn from(value: bool) -> Self {
        if value {
            Self(1)
        } else {
            Self(0)
        }
    }
}

impl From<PlcBool> for bool {
    fn from(value: PlcBool) -> Self {
        if value.0 == 1 {
            true
        } else if value.0 == 0 {
            false
        } else {
            panic!("Unexpected: Bool can only be 0 or 1!")
        }
    }
}
