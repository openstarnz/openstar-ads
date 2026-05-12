use crate::data_types::AdsData;

#[derive(Clone, Debug, Default, zerocopy::Immutable, zerocopy::IntoBytes, zerocopy::FromBytes)]
#[repr(C)]
pub struct AdsUint(u16);

impl AdsData for AdsUint {}

impl From<u16> for AdsUint {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<AdsUint> for u16 {
    fn from(value: AdsUint) -> Self {
        value.0
    }
}
