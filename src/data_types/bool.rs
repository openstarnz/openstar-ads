use crate::data_types::AdsData;

#[derive(Clone, Debug, Default, zerocopy::Immutable, zerocopy::IntoBytes, zerocopy::FromBytes)]
#[repr(C)]
pub struct AdsBool(u8);

impl AdsData for AdsBool {}

impl From<bool> for AdsBool {
    fn from(value: bool) -> Self {
        if value {
            Self(1)
        } else {
            Self(0)
        }
    }
}

impl From<AdsBool> for bool {
    fn from(value: AdsBool) -> Self {
        if value.0 == 1 {
            true
        } else if value.0 == 0 {
            false
        } else {
            panic!("Unexpected: Bool can only be 0 or 1!")
        }
    }
}
