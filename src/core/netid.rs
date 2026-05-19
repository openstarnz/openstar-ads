//! Contains the AMS NetId and related types.

use std::convert::TryInto;
use std::fmt::{self, Display};
use std::net::Ipv4Addr;
use std::num::ParseIntError;
use std::str::FromStr;

use itertools::Itertools;
use zerocopy::{FromBytes, Immutable, IntoBytes};

/// Represents an AMS NetID.
///
/// The NetID consists of 6 bytes commonly written like an IPv4 address, i.e.
/// `1.2.3.4.5.6`. Together with an AMS port (16-bit integer), it uniquely
/// identifies an endpoint of an ADS system that can be communicated with.
///
/// Although often the first 4 bytes of a NetID look like an IP address, and
/// sometimes even are identical to the device's IP address, there is no
/// requirement for this, and one should never rely on it.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Debug, FromBytes, IntoBytes, Immutable,
)]
#[repr(C)]
pub struct AmsNetId(pub [u8; 6]);

/// An AMS port is, similar to an IP port, a 16-bit integer.
pub type AmsPort = u16;

impl AmsNetId {
    /// Create a NetID from six bytes.
    pub const fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> Self {
        AmsNetId([a, b, c, d, e, f])
    }

    /// Return the "local NetID", `127.0.0.1.1.1`.
    pub const fn local() -> Self {
        AmsNetId([127, 0, 0, 1, 1, 1])
    }

    /// Create a NetID from a slice (which must have length 6).
    pub fn from_slice(slice: &[u8]) -> Option<Self> {
        Some(AmsNetId(slice.try_into().ok()?))
    }

    /// Create a NetID from an IPv4 address and two additional octets.
    pub fn from_ip(ip: Ipv4Addr, e: u8, f: u8) -> Self {
        let [a, b, c, d] = ip.octets();
        Self::new(a, b, c, d, e, f)
    }

    /// Check if the NetID is all-zero.
    pub fn is_zero(&self) -> bool {
        self.0 == [0, 0, 0, 0, 0, 0]
    }
}

/// Error when parsing an AMS NetID from a string
#[derive(Debug, Clone, thiserror::Error)]
pub enum ParseAmsNetIdError {
    #[error("failed to parse byte: {0}")]
    ParseByte(#[from] ParseIntError),

    #[error("AmsNetId consists of exactly 6 bytes")]
    Not6Bytes,
}

impl FromStr for AmsNetId {
    type Err = ParseAmsNetIdError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let mut bytes = Vec::with_capacity(6);
        for (index, item) in s.split('.').enumerate() {
            let byte = item.parse::<u8>()?;
            bytes[index] = byte;
        }
        let bytes: [u8; 6] = bytes
            .try_into()
            .map_err(|_error| ParseAmsNetIdError::Not6Bytes)?;
        Ok(Self(bytes))
    }
}

impl From<[u8; 6]> for AmsNetId {
    fn from(array: [u8; 6]) -> Self {
        Self(array)
    }
}

impl Display for AmsNetId {
    /// Format a NetID in the usual format.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.iter().format("."))
    }
}

/// Combination of an AMS NetID and a port.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct AmsAddr(AmsNetId, AmsPort);

impl AmsAddr {
    /// Create a new address from NetID and port.
    pub const fn new(netid: AmsNetId, port: AmsPort) -> Self {
        Self(netid, port)
    }

    /// Return the NetID of this address.
    pub const fn netid(&self) -> AmsNetId {
        self.0
    }

    /// Return the port of this address.
    pub const fn port(&self) -> AmsPort {
        self.1
    }
}

/// Error when parsing an AMS address from a string
#[derive(Debug, Clone, thiserror::Error)]
pub enum ParseAmsAddrError {
    #[error("invalid AMS addr string: {input}")]
    InvalidInput { input: String },

    #[error("failed to parse AMS NetId: {0}")]
    ParseAmsNetId(#[from] ParseAmsNetIdError),

    #[error("invalid port number")]
    InvalidPortNumber(ParseIntError),
}

impl FromStr for AmsAddr {
    type Err = ParseAmsAddrError;

    /// Parse an AMS address from a string (netid:port).
    fn from_str(s: &str) -> Result<AmsAddr, Self::Err> {
        let (addr, port) =
            s.split(':')
                .collect_tuple()
                .ok_or_else(|| ParseAmsAddrError::InvalidInput {
                    input: s.to_owned(),
                })?;
        let addr = addr.parse()?;
        let port = port.parse().map_err(ParseAmsAddrError::InvalidPortNumber)?;
        Ok(Self(addr, port))
    }
}

impl Display for AmsAddr {
    /// Format an AMS address in the usual format.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.0, self.1)
    }
}
