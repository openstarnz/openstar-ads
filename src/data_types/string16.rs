use thiserror::Error;
/**
 * A latin-1 16 Character String for Beckhoff ADS. Includes a 0 for null termination.
 */
use zerocopy::FromZeroes;

use crate::data_types::AdsData;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct AdsString16 {
    inner: [u8; 16],
    null_terminator: u8, // The ADS spec includes one byte of null termination
}

impl AdsData for AdsString16 {}

#[derive(Debug, Error, Clone)]
pub enum AdsString16FromStringError {
    #[error("Could not convert from String. Longer than 16 bytes.")]
    LongerThan16Bytes,
}

impl TryFrom<String> for AdsString16 {
    type Error = AdsString16FromStringError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let src = value.as_bytes();
        let src_len = src.len();
        if src_len > 16 {
            return Err(AdsString16FromStringError::LongerThan16Bytes);
        }
        let mut blank = AdsString16::new_zeroed();
        // Does not use from_bytes as it does not need the length to match perfectly
        blank.inner[..src_len].copy_from_slice(src);
        Ok(blank)
    }
}

impl From<AdsString16> for String {
    fn from(val: AdsString16) -> Self {
        val.inner
            .into_iter()
            .map(|c| c as char)
            .collect::<String>()
            .trim_end_matches("\0")
            .into()
    }
}

#[derive(Debug, Error, Clone)]
#[error("Null terminator is {null_terminator} not 0.")]
pub struct AdsString16CheckTerminatorError {
    null_terminator: u8,
}

impl AdsString16 {
    // Returns an error if the null terminator is not zero.
    pub fn check_terminator(&self) -> Result<(), AdsString16CheckTerminatorError> {
        if self.null_terminator != 0 {
            Err(AdsString16CheckTerminatorError {
                null_terminator: self.null_terminator,
            })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_16_character_string() {
        let input_string = String::from("Status: Healthy!");

        let plc_string = AdsString16::try_from(input_string.clone())
            .expect("Unexpected: could not get plc string from valid string");

        let output_string: String = plc_string.into();

        assert_eq!(input_string, output_string);
    }

    #[test]
    fn convert_15_character_string() {
        let input_string = String::from("Status: Healthy");

        let plc_string = AdsString16::try_from(input_string.clone())
            .expect("Unexpected: could not get plc string from valid string");

        let output_string: String = plc_string.into();

        assert_eq!(input_string, output_string);
    }

    #[test]
    fn convert_17_character_string() {
        let input_string = String::from("Status: Healthy!!");

        let plc_string_result = AdsString16::try_from(input_string.clone());

        assert!(plc_string_result.is_err());
    }

    #[test]
    fn convert_empty_string() {
        let input_string = String::new();

        let plc_string = AdsString16::try_from(input_string.clone())
            .expect("Unexpected: could not get plc string from valid string");

        let output_string: String = plc_string.into();

        assert_eq!(input_string, output_string);
    }

    #[test]
    fn convert_string_with_internal_null_characters() {
        let input_string = String::from("Status:\0\0Healthy");

        let plc_string = AdsString16::try_from(input_string.clone())
            .expect("Unexpected: could not get plc string from valid string");

        let output_string: String = plc_string.into();

        assert_eq!(input_string, output_string);
    }
}
