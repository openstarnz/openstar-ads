use crate::data_types::AdsData;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct AdsDint(i32);

impl AdsData for AdsDint {}

impl From<AdsDint> for i32 {
    fn from(value: AdsDint) -> Self {
        value.0
    }
}
