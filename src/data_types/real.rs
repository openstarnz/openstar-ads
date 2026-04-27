use crate::data_types::AdsData;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct AdsReal(pub f32);

impl AdsData for AdsReal {}

impl From<f32> for AdsReal {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

impl From<AdsReal> for f32 {
    fn from(value: AdsReal) -> Self {
        value.0
    }
}
