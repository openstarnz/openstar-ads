use chrono::{DateTime, NaiveDate, Utc};

use crate::data_types::PlcDataType;

use super::word::PlcWord;

#[derive(Clone, Debug, Default, zerocopy::AsBytes, zerocopy::FromBytes, zerocopy::FromZeroes)]
#[repr(C)]
pub struct PlcTimeStruct {
    year: PlcWord,         // wYear: the year: 1970 ~ 2106;
    month: PlcWord,        // wMonth: the month: 1 ~ 12 (January = 1, February = 2, etc.);
    day_of_week: PlcWord,  // wDayOfWeek: the day of the week: 0 ~ 6 (Sunday = 0, Monday = 1 etc. );
    day: PlcWord,          // wDay: the day of the month: 1 ~ 31;
    hour: PlcWord,         // wHour: hour: 0 ~ 23;
    minute: PlcWord,       // wMinute: minute: 0 ~ 59;
    second: PlcWord,       // wSecond: second: 0 ~ 59;
    milliseconds: PlcWord, // wMilliseconds: millisecond: 0 ~ 999;
}

impl PlcDataType for PlcTimeStruct {}

impl From<PlcTimeStruct> for Option<DateTime<Utc>> {
    fn from(value: PlcTimeStruct) -> Self {
        let year: u16 = value.year.into();
        let month: u16 = value.month.into();
        let day: u16 = value.day.into();

        let hour: u16 = value.hour.into();
        let minute: u16 = value.minute.into();
        let second: u16 = value.second.into();
        let milliseconds: u16 = value.milliseconds.into();

        NaiveDate::from_ymd_opt(year.into(), month.into(), day.into()).and_then(|date| {
            date.and_hms_milli_opt(
                hour.into(),
                minute.into(),
                second.into(),
                milliseconds.into(),
            )
            .map(|datetime| datetime.and_utc())
        })
    }
}
