mod client;
pub mod index;
mod netid;
mod notification;
pub mod protocol;
mod strings;

pub use self::client::{Client, Timeouts};
pub use self::netid::{AmsAddr, AmsNetId, AmsPort, ParseAmsAddrError, ParseAmsNetIdError};
pub use self::notification::NotificationTransmissionMode;
pub(crate) use self::notification::{NotificationAttributes, NotificationHandle};
pub use self::protocol::{AdsState, DeviceInfo};
pub use self::strings::{String, String80, WString, WString80};

/// The default port for TCP communication.
pub const PORT: u16 = 0xBF02;
/// The default port for UDP communication.
pub const UDP_PORT: u16 = 0xBF03;
