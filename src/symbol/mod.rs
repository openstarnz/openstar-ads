//! How we handle dynamic symbol types at runtime.

mod map;
mod tree;
mod r#type;
mod type_tree;

pub use self::map::*;
pub use self::r#type::*;
pub use self::tree::*;
pub use self::type_tree::*;

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Datelike, Timelike, Utc};
    use indexmap::IndexMap;
    use serde::Deserialize;

    use crate::symbol::{
        PrimitiveValue, SymbolMap, SymbolMapExt, SymbolTree, SymbolTypeMap, SymbolTypeMapExt,
        SymbolTypeTree,
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

        let symbol_map = SymbolMap::from_bytes(&data, &symbol_type_map);

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

        let symbol_map = SymbolMap::from_bytes(&data, &symbol_type_map);

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

    #[test]
    fn test_boolean() {
        let size = 2;
        let mut boolean_symbol_map: IndexMap<String, (SymbolTypeTree, Option<u32>)> =
            IndexMap::new();
        let mut data: Vec<u8> = Vec::new();
        boolean_symbol_map.insert("true".to_string(), (SymbolTypeTree::Bool, Some(0)));
        boolean_symbol_map.insert("false".to_string(), (SymbolTypeTree::Bool, Some(1)));

        data.append(&mut (1u8).to_le_bytes().to_vec());
        data.append(&mut (0u8).to_le_bytes().to_vec());

        let symbol_type_tree = SymbolTypeTree::Struct(boolean_symbol_map, size);

        let symbol_type_map = SymbolTypeMap::from_tree(symbol_type_tree, 0, "".to_string());

        let symbol_map = SymbolMap::from_bytes(&data, &symbol_type_map);

        let expected = indexmap::indexmap! {
            "true".to_string() => PrimitiveValue::Bool(true),
            "false".to_string() => PrimitiveValue::Bool(false),
        };

        assert_eq!(symbol_map, expected);
    }

    #[test]
    fn test_strings() {
        let size = 36;
        let mut strings_symbol_map: IndexMap<String, (SymbolTypeTree, Option<u32>)> =
            IndexMap::new();
        let mut data: Vec<u8> = Vec::new();
        strings_symbol_map.insert("utf8".to_string(), (SymbolTypeTree::String(11), Some(0)));
        strings_symbol_map.insert("utf16".to_string(), (SymbolTypeTree::Wstring(24), Some(12)));

        data.append(&mut "Test string".as_bytes().to_vec());
        // Buffer byte for alignment
        data.push(0u8);
        data.append(
            &mut "Test string!"
                .encode_utf16()
                .flat_map(|word| word.to_le_bytes())
                .collect(),
        );

        let symbol_type_tree = SymbolTypeTree::Struct(strings_symbol_map, size);

        let symbol_type_map = SymbolTypeMap::from_tree(symbol_type_tree, 0, "".to_string());

        let symbol_map = SymbolMap::from_bytes(&data, &symbol_type_map);

        let expected = indexmap::indexmap! {
            "utf8".to_string() => PrimitiveValue::String("Test string".to_string()),
            "utf16".to_string() => PrimitiveValue::String("Test string!".to_string()),
        };

        assert_eq!(symbol_map, expected);
    }

    #[test]
    fn test_reals() {
        let size = 16;
        let mut numbers_symbol_map: IndexMap<String, (SymbolTypeTree, Option<u32>)> =
            IndexMap::new();
        let mut data: Vec<u8> = Vec::new();
        numbers_symbol_map.insert("f64".to_string(), (SymbolTypeTree::Lreal, Some(0)));
        numbers_symbol_map.insert("f32".to_string(), (SymbolTypeTree::Real, Some(8)));

        data.append(&mut 1.23f64.to_le_bytes().to_vec());
        data.append(&mut 4.56f32.to_le_bytes().to_vec());

        let symbol_type_tree = SymbolTypeTree::Struct(numbers_symbol_map, size);

        let symbol_type_map = SymbolTypeMap::from_tree(symbol_type_tree, 0, "".to_string());

        let symbol_map = SymbolMap::from_bytes(&data, &symbol_type_map);

        let expected = indexmap::indexmap! {
            "f64".to_string() => PrimitiveValue::Float(1.23f64),
            "f32".to_string() => PrimitiveValue::Float(4.56f32 as f64),
        };

        assert_eq!(symbol_map, expected);
    }

    #[test]
    fn test_void() {
        let size = 12;
        let mut void_symbol_map: IndexMap<String, (SymbolTypeTree, Option<u32>)> = IndexMap::new();
        let mut data: Vec<u8> = Vec::new();
        void_symbol_map.insert("u128".to_string(), (SymbolTypeTree::Void(16), Some(0)));

        data.append(
            // Designed to be bytes ascending from 1 to 16
            &mut 21345817372864405881847059188222722561u128
                .to_le_bytes()
                .to_vec(),
        );

        let symbol_type_tree = SymbolTypeTree::Struct(void_symbol_map, size);

        let symbol_type_map = SymbolTypeMap::from_tree(symbol_type_tree, 0, "".to_string());

        let symbol_map = SymbolMap::from_bytes(&data, &symbol_type_map);

        let expected = indexmap::indexmap! {
            "u128".to_string() => PrimitiveValue::Void(vec![1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8, 9u8, 10u8, 11u8, 12u8, 13u8, 14u8, 15u8, 16u8])
        };

        assert_eq!(symbol_map, expected);
    }

    #[test]
    fn deserialize_struct_with_multiple_fields() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct TwoFields {
            a: u16,
            b: u16,
        }

        let mut map = IndexMap::new();
        map.insert("a".into(), SymbolTree::Uint(1));
        map.insert("b".into(), SymbolTree::Uint(2));
        let tree = SymbolTree::Struct(map);

        let result = TwoFields::deserialize(&tree).unwrap();
        assert_eq!(result, TwoFields { a: 1, b: 2 });
    }

    #[test]
    fn deserialize_missing_as_none() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct S {
            x: Option<u16>,
        }

        let mut map = IndexMap::new();
        map.insert("x".into(), SymbolTree::Missing);
        let tree = SymbolTree::Struct(map);

        let result = S::deserialize(&tree).unwrap();
        assert_eq!(result, S { x: None });
    }

    #[test]
    fn deserialize_present_option() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct S {
            x: Option<u16>,
        }

        let mut map = IndexMap::new();
        map.insert("x".into(), SymbolTree::Uint(5));
        let tree = SymbolTree::Struct(map);

        let result = S::deserialize(&tree).unwrap();
        assert_eq!(result, S { x: Some(5) });
    }

    #[test]
    fn deserialize_newtype_struct() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct Wrapper(u16);

        let tree = SymbolTree::Uint(42);
        let result = Wrapper::deserialize(&tree).unwrap();
        assert_eq!(result, Wrapper(42));
    }

    #[test]
    fn deserialize_array_seq() {
        let tree = SymbolTree::Array(vec![SymbolTree::Uint(1), SymbolTree::Uint(2)]);
        let v: Vec<u16> = Vec::deserialize(&tree).unwrap();
        assert_eq!(v, vec![1, 2]);
    }
}
