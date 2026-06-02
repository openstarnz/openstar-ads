//! # openstar-ads
//!
//! An async ADS client to interact with Beckhoff PLCs, as needed by [OpenStar].
//!
//! Features:
//!
//! - Asynchronous, non-blocking, integrated with [`tokio`][tokio]
//! - Easy connections (with or without ADS router)
//! - Read symbol values
//! - Subscribe to symbol values
//! - Call RPC methods, with optional params and optional output
//! - Get dynamic symbol type trees at runtime
//! - Subscribe to a symbol tree using the dynamic type tree
//! - Subscribe to a flattened map of data for dynamic type tree
//!
//! Based on the [`ads`][ads] crate.
//!
//! [OpenStar]: https://openstar.tech
//! [tokio]: https://tokio.rs/
//! [ads]: https://github.com/birkenfeld/ads-rs
//!
//! ## Installation
//!
//! Add to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! openstar-ads = { git = "git@github.com:openstarnz/openstar-ads" }
//! ```
//!
//! ## Usage
//!
//! The main interface is [`Ads`].
//!
//! ```rust,no_run
//! use anyhow::Context;
//! use openstar_ads::{AdsBuilder, AmsAddr, NotificationTransmissionMode};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let router_ip = env_var("EXAMPLE_ADS_ROUTER_ADDRESS")?;
//!     let router_port = env_var("EXAMPLE_ADS_ROUTER_PORT")?;
//!     let target_net_id = env_var("EXAMPLE_PLC_AMS_NET_ID")?;
//!     let target_port = env_var("EXAMPLE_PLC_AMS_PORT")?;
//!     let source_net_id = env_var("EXAMPLE_LOCAL_AMS_NET_ID")?;
//!     let source_port = env_var("EXAMPLE_LOCAL_AMS_PORT")?;
//!
//!     let router_addr: (String, u16) = (router_ip, router_port.parse()?);
//!     let target_addr = AmsAddr::new(target_net_id.parse()?, target_port.parse()?);
//!     let source_addr = AmsAddr::new(source_net_id.parse()?, source_port.parse()?);
//!
//!     let ads = AdsBuilder::new(target_addr)
//!         .router(router_addr)
//!         .source(source_addr)
//!         .build()
//!         .await
//!         .unwrap();
//!
//!     let symbol_type_tree = ads.get_dynamic_type_tree("MAIN.example").await?;
//!     let symbol_type_tree = symbol_type_tree.get_child("status").clone();
//!
//!     let status_symbol = "MAIN.example.status";
//!     let mut subscription = ads.subscribe_map(
//!         status_symbol,
//!         symbol_type_tree,
//!         NotificationTransmissionMode::ServerOnChange,
//!     ).await?;
//!
//!     while let Some(_value) = subscription.recv().await {
//!         // Do stuff
//!     }
//!
//!     Ok(())
//! }
//!
//! fn env_var(key: &str) -> anyhow::Result<String> {
//!     std::env::var(key)
//!         .with_context(|| format!("Environment variable {key} not found."))
//! }
//! ```

mod ads;
mod connection;
pub(crate) mod core;
pub mod data_types;
mod error;
mod params;
pub mod symbol;

pub use self::ads::*;
pub(crate) use self::connection::*;
pub use self::core::{
    AdsState, AmsAddr, AmsNetId, AmsPort, DeviceInfo, NotificationTransmissionMode,
    ParseAmsAddrError, ParseAmsNetIdError, Timeouts, PORT, UDP_PORT,
};
pub use self::error::*;
pub use self::params::*;
