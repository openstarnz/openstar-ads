//! ads-client is a wrapper around the ads rust crate to improve the management of PLC connections and symbol subscriptions.

/// data_types contains types to help deserialise symbol information from ADS into usable Rust types.
pub mod data_types;
/// plc_client wraps around the ads Client features to provide easier abstractions.
pub mod plc_client;
/// plc_connection provides an async safe wrapper around the plc_client connection.
pub mod plc_connection;

mod params;
mod symbol;

pub use self::params::*;
pub use self::symbol::*;
