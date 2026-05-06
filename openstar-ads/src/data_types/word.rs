use crate::data_types::AdsData;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct AdsWord(u16);

impl AdsData for AdsWord {}

impl From<AdsWord> for u16 {
    fn from(value: AdsWord) -> Self {
        value.0
    }
}
