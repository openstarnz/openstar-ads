use crate::data_types::AdsData;

#[derive(Clone, Debug, Default, zerocopy::Immutable, zerocopy::IntoBytes, zerocopy::FromBytes)]
#[repr(C)]
pub struct AdsUdint(u32);

impl AdsData for AdsUdint {}

impl From<[u16; 2]> for AdsUdint {
    fn from(value: [u16; 2]) -> Self {
        Self(bytemuck::cast(value))
    }
}

impl From<u32> for AdsUdint {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<AdsUdint> for u32 {
    fn from(value: AdsUdint) -> Self {
        value.0
    }
}
