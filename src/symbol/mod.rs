mod map;
mod tree;
mod type_tree;

pub use self::map::*;
pub use self::tree::*;
pub use self::type_tree::*;

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Datelike, Timelike, Utc};
    use indexmap::IndexMap;

    use crate::{
        PrimitiveValue, SymbolMap, SymbolMapExt, SymbolTypeMap, SymbolTypeMapExt, SymbolTypeTree,
    };

    #[test]
    fn test_integers() {
        let size = 12;
        let mut numbers_symbol_map: IndexMap<String, (SymbolTypeTree, Option<u32>)> =
            IndexMap::new();
        let mut data: Vec<u8> = Vec::new();
        numbers_symbol_map.insert("integer".to_string(), (SymbolTypeTree::Int, Some(0)));
        numbers_symbol_map.insert("uinteger".to_string(), (SymbolTypeTree::Uint, Some(2)));
        numbers_symbol_map.insert("dinteger".to_string(), (SymbolTypeTree::Dint, Some(4)));
        numbers_symbol_map.insert("udinteger".to_string(), (SymbolTypeTree::Udint, Some(8)));
        numbers_symbol_map.insert("linteger".to_string(), (SymbolTypeTree::Lint, Some(16)));
        numbers_symbol_map.insert("ulinteger".to_string(), (SymbolTypeTree::Ulint, Some(24)));

        data.append(&mut (-123_i16).to_le_bytes().to_vec());
        data.append(&mut 123u16.to_le_bytes().to_vec());
        data.append(&mut (-123456_i32).to_le_bytes().to_vec());
        data.append(&mut 123456u32.to_le_bytes().to_vec());
        data.append(&mut 0u32.to_le_bytes().to_vec());
        data.append(&mut (-123456789_i64).to_le_bytes().to_vec());
        data.append(&mut 123456789u64.to_le_bytes().to_vec());

        let symbol_type_tree = SymbolTypeTree::Struct(numbers_symbol_map, size);

        let symbol_type_map = SymbolTypeMap::from_tree(symbol_type_tree, 0, "".to_string());

        let symbol_map = SymbolMap::from_bytes(&symbol_type_map, &data);

        let expected = indexmap::indexmap! {
            "integer".to_string() => PrimitiveValue::Int(-123),
            "uinteger".to_string() => PrimitiveValue::Uint(123),
            "dinteger".to_string() => PrimitiveValue::Int(-123456),
            "udinteger".to_string() => PrimitiveValue::Uint(123456),
            "linteger".to_string() => PrimitiveValue::Int(-123456789),
            "ulinteger".to_string() => PrimitiveValue::Uint(123456789),
        };

        assert_eq!(symbol_map, expected);
    }

    #[test]
    fn test_date_time() {
        let size = 14;
        let mut numbers_symbol_map: IndexMap<String, (SymbolTypeTree, Option<u32>)> =
            IndexMap::new();
        let mut data: Vec<u8> = Vec::new();

        let year: u16 = 2026;
        let month: u16 = 3;
        let day: u16 = 26;
        let hour: u16 = 4;
        let minute: u16 = 15;
        let second: u16 = 12;
        let milliseconds: u16 = 987;

        numbers_symbol_map.insert("wYear".to_string(), (SymbolTypeTree::Uint, Some(0)));
        numbers_symbol_map.insert("wMonth".to_string(), (SymbolTypeTree::Uint, Some(2)));
        numbers_symbol_map.insert("wDayOfWeek".to_string(), (SymbolTypeTree::Uint, Some(4)));
        numbers_symbol_map.insert("wDay".to_string(), (SymbolTypeTree::Uint, Some(6)));
        numbers_symbol_map.insert("wHour".to_string(), (SymbolTypeTree::Uint, Some(8)));
        numbers_symbol_map.insert("wMinute".to_string(), (SymbolTypeTree::Uint, Some(10)));
        numbers_symbol_map.insert("wSecond".to_string(), (SymbolTypeTree::Uint, Some(12)));
        numbers_symbol_map.insert(
            "wMilliseconds".to_string(),
            (SymbolTypeTree::Uint, Some(14)),
        );

        data.append(&mut year.to_le_bytes().to_vec());
        data.append(&mut month.to_le_bytes().to_vec());
        data.append(&mut 4u16.to_le_bytes().to_vec());
        data.append(&mut day.to_le_bytes().to_vec());
        data.append(&mut hour.to_le_bytes().to_vec());
        data.append(&mut minute.to_le_bytes().to_vec());
        data.append(&mut second.to_le_bytes().to_vec());
        data.append(&mut milliseconds.to_le_bytes().to_vec());

        let symbol_type_tree = SymbolTypeTree::Struct(
            indexmap::indexmap! {"timestamp".to_string() => (SymbolTypeTree::Struct(numbers_symbol_map, size), Some(0))},
            size,
        );

        let symbol_type_map = SymbolTypeMap::from_tree(symbol_type_tree, 0, "status".to_string());

        let symbol_map = SymbolMap::from_bytes(&symbol_type_map, &data);

        let expected_date: DateTime<Utc> = DateTime::from_timestamp_millis(milliseconds.into())
            .unwrap()
            .with_year(year.into())
            .unwrap()
            .with_month(month.into())
            .unwrap()
            .with_day(day.into())
            .unwrap()
            .with_hour(hour.into())
            .unwrap()
            .with_minute(minute.into())
            .unwrap()
            .with_second(second.into())
            .unwrap();

        let expected = indexmap::indexmap! {
            "status.timestamp".to_string() => PrimitiveValue::Timestamp(expected_date),
        };

        assert_eq!(symbol_map, expected);
    }
}
