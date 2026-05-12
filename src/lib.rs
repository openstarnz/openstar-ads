//! openstar-ads is a wrapper around the `ads_client` Rust crate to improve the management of ADS connections and symbol subscriptions.

mod ads;
mod client;
mod connection;
pub mod core;
mod data_types;
mod error;
mod params;
mod symbol;

pub use self::ads::*;
pub use self::client::NotificationSubscription;
pub(crate) use self::client::*;
pub(crate) use self::connection::*;
pub use self::data_types::*;
pub use self::error::*;
pub use self::params::*;
pub use self::symbol::*;
