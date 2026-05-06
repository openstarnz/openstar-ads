use crate::data_types::AdsData;

use super::udint::AdsUdint;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct AdsTime(AdsUdint);

impl AdsData for AdsTime {}

impl From<u32> for AdsTime {
    fn from(value: u32) -> Self {
        Self(AdsUdint::from(value))
    }
}

impl From<AdsTime> for u32 {
    fn from(value: AdsTime) -> Self {
        value.0.into()
    }
}
