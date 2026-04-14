/**
 * A latin-1 16 Character String for Beckhoff ADS. Includes a 0 for null termination.
 */
use anyhow::{anyhow, bail};
use zerocopy::FromZeroes;

use crate::data_types::PlcDataType;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct PlcString16 {
    inner: [u8; 16],
    null_terminator: u8, // The PLC spec includes one byte of null termination
}

impl PlcDataType for PlcString16 {}

impl TryFrom<String> for PlcString16 {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let src = value.as_bytes();
        let src_len = src.len();
        if src_len > 16 {
            return Err(anyhow!(
                "Could not convert from String. Longer than 16 bytes."
            ));
        }
        let mut blank = PlcString16::new_zeroed();
        // Does not use from_bytes as it does not need the length to match perfectly
        blank.inner[..src_len].copy_from_slice(src);
        Ok(blank)
    }
}

impl From<PlcString16> for String {
    fn from(val: PlcString16) -> Self {
        val.inner
            .into_iter()
            .map(|c| c as char)
            .collect::<String>()
            .trim_end_matches("\0")
            .into()
    }
}

impl PlcString16 {
    // Returns an error if the null terminator is not zero.
    pub fn check_terminator(&self) -> anyhow::Result<()> {
        if self.null_terminator != 0 {
            bail!("Null terminator is {} not 0.", self.null_terminator);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_16_character_string() {
        let input_string = String::from("Status: Healthy!");

        let plc_string = PlcString16::try_from(input_string.clone())
            .expect("Unexpected: could not get plc string from valid string");

        let output_string: String = plc_string.into();

        assert_eq!(input_string, output_string);
    }

    #[test]
    fn convert_15_character_string() {
        let input_string = String::from("Status: Healthy");

        let plc_string = PlcString16::try_from(input_string.clone())
            .expect("Unexpected: could not get plc string from valid string");

        let output_string: String = plc_string.into();

        assert_eq!(input_string, output_string);
    }

    #[test]
    fn convert_17_character_string() {
        let input_string = String::from("Status: Healthy!!");

        let plc_string_result = PlcString16::try_from(input_string.clone());

        assert!(plc_string_result.is_err());
    }

    #[test]
    fn convert_empty_string() {
        let input_string = String::new();

        let plc_string = PlcString16::try_from(input_string.clone())
            .expect("Unexpected: could not get plc string from valid string");

        let output_string: String = plc_string.into();

        assert_eq!(input_string, output_string);
    }

    #[test]
    fn convert_string_with_internal_null_characters() {
        let input_string = String::from("Status:\0\0Healthy");

        let plc_string = PlcString16::try_from(input_string.clone())
            .expect("Unexpected: could not get plc string from valid string");

        let output_string: String = plc_string.into();

        assert_eq!(input_string, output_string);
    }
}
