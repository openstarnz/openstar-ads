use crate::data_types::AdsData;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct AdsInt(i16);

impl AdsData for AdsInt {}

impl From<i16> for AdsInt {
    fn from(value: i16) -> Self {
        Self(value)
    }
}

impl From<AdsInt> for i16 {
    fn from(value: AdsInt) -> Self {
        value.0
    }
}
