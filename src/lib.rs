//! ads-client is a wrapper around the ads rust crate to improve the management of PLC connections and symbol subscriptions.

/// plc_client wraps around the ads Client features to provide easier abstractions.
pub mod plc;

/// data_types contains types to help deserialise symbol information from ADS into usable Rust types.
mod data_types;

/// plc_connection provides an async safe wrapper around the plc_client connection.
pub mod plc_connection;

pub(crate) mod client;
pub(crate) mod connection;
mod error;
mod params;
mod symbol;

pub use self::data_types::*;
pub use self::error::*;
pub use self::params::*;
pub use self::symbol::*;
