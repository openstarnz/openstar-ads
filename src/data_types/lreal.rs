use crate::data_types::AdsData;

#[derive(Clone, Debug, Default, zerocopy::Immutable, zerocopy::IntoBytes, zerocopy::FromBytes)]
#[repr(C)]
pub struct AdsLreal(f64);

impl AdsData for AdsLreal {}

impl From<f64> for AdsLreal {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

impl From<AdsLreal> for f64 {
    fn from(value: AdsLreal) -> Self {
        value.0
    }
}
