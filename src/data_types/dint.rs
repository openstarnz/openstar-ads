use crate::data_types::AdsData;

#[derive(Clone, Debug, Default, zerocopy::Immutable, zerocopy::IntoBytes, zerocopy::FromBytes)]
#[repr(C)]
pub struct AdsDint(i32);

impl AdsData for AdsDint {}

impl From<AdsDint> for i32 {
    fn from(value: AdsDint) -> Self {
        value.0
    }
}
