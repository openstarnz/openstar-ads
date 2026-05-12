use chrono::{DateTime, NaiveDate, Utc};

use crate::data_types::AdsData;

use super::word::AdsWord;

#[derive(Clone, Debug, Default, zerocopy::Immutable, zerocopy::IntoBytes, zerocopy::FromBytes)]
#[repr(C)]
pub struct AdsTimeStruct {
    year: AdsWord,         // wYear: the year: 1970 ~ 2106;
    month: AdsWord,        // wMonth: the month: 1 ~ 12 (January = 1, February = 2, etc.);
    day_of_week: AdsWord,  // wDayOfWeek: the day of the week: 0 ~ 6 (Sunday = 0, Monday = 1 etc. );
    day: AdsWord,          // wDay: the day of the month: 1 ~ 31;
    hour: AdsWord,         // wHour: hour: 0 ~ 23;
    minute: AdsWord,       // wMinute: minute: 0 ~ 59;
    second: AdsWord,       // wSecond: second: 0 ~ 59;
    milliseconds: AdsWord, // wMilliseconds: millisecond: 0 ~ 999;
}

impl AdsData for AdsTimeStruct {}

impl From<AdsTimeStruct> for Option<DateTime<Utc>> {
    fn from(value: AdsTimeStruct) -> Self {
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
