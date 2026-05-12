pub mod client;
pub mod index;
pub mod netid;
pub mod notif;
pub mod protocol;
pub mod strings;

pub use client::{Client, ClientBuilder, Timeouts};
pub use netid::{AmsAddr, AmsNetId, AmsPort};
pub use protocol::AdsState;

/// The default port for TCP communication.
pub const PORT: u16 = 0xBF02;
/// The default port for UDP communication.
pub const UDP_PORT: u16 = 0xBF03;
