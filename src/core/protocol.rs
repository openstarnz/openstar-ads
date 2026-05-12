use std::str::FromStr;
use zerocopy::{
    little_endian::{U16, U32},
    FromBytes, Immutable, IntoBytes,
};

use crate::AmsNetId;

/// Size of the AMS/TCP + AMS headers
// https://infosys.beckhoff.com/content/1033/tc3_ads_intro/115845259.html?id=6032227753916597086
pub(crate) const AMS_TCP_HEADER_SIZE: usize = 6;
pub(crate) const AMS_HEADER_SIZE: usize = 32;
pub(crate) const ADS_HEADER_SIZE: usize = AMS_TCP_HEADER_SIZE + AMS_HEADER_SIZE; // including AMS/TCP header

/// An ADS protocol command.
// https://infosys.beckhoff.com/content/1033/tc3_ads_intro/115847307.html?id=7738940192708835096
#[repr(u16)]
#[derive(Clone, Copy, Debug)]
pub enum Command {
    /// Return device info
    DevInfo = 1,
    /// Read some data
    Read = 2,
    /// Write some data
    Write = 3,
    /// Write some data, then read back some data
    /// (used as a poor-man's function call)
    ReadWrite = 9,
    /// Read the ADS and device state
    ReadState = 4,
    /// Set the ADS and device state
    WriteControl = 5,
    /// Add a notification for a given index
    AddNotification = 6,
    /// Add a notification for a given index
    DeleteNotification = 7,
    /// Change occurred in a given notification,
    /// can be sent by the PLC only
    Notification = 8,
}

impl Command {
    pub fn action(self) -> &'static str {
        match self {
            Command::DevInfo => "get device info",
            Command::Read => "read data",
            Command::Write => "write data",
            Command::ReadWrite => "write and read data",
            Command::ReadState => "read state",
            Command::WriteControl => "write control",
            Command::AddNotification => "add notification",
            Command::DeleteNotification => "delete notification",
            Command::Notification => "notification",
        }
    }
}

/// Device info returned from an ADS server.
#[derive(Debug)]
pub struct DeviceInfo {
    /// Name of the ADS device/service.
    pub name: String,
    /// Major version.
    pub major: u8,
    /// Minor version.
    pub minor: u8,
    /// Build version.
    pub version: u16,
}

/// The ADS state of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
#[repr(u16)]
pub enum AdsState {
    Invalid = 0,
    Idle = 1,
    Reset = 2,
    Init = 3,
    Start = 4,
    Run = 5,
    Stop = 6,
    SaveCfg = 7,
    LoadCfg = 8,
    PowerFail = 9,
    PowerGood = 10,
    Error = 11,
    Shutdown = 12,
    Suspend = 13,
    Resume = 14,
    Config = 15,
    Reconfig = 16,
    Stopping = 17,
    Incompatible = 18,
    Exception = 19,
}

impl TryFrom<u16> for AdsState {
    type Error = &'static str;

    fn try_from(value: u16) -> std::result::Result<Self, &'static str> {
        Ok(match value {
            0 => Self::Invalid,
            1 => Self::Idle,
            2 => Self::Reset,
            3 => Self::Init,
            4 => Self::Start,
            5 => Self::Run,
            6 => Self::Stop,
            7 => Self::SaveCfg,
            8 => Self::LoadCfg,
            9 => Self::PowerFail,
            10 => Self::PowerGood,
            11 => Self::Error,
            12 => Self::Shutdown,
            13 => Self::Suspend,
            14 => Self::Resume,
            15 => Self::Config,
            16 => Self::Reconfig,
            17 => Self::Stopping,
            18 => Self::Incompatible,
            19 => Self::Exception,
            _ => return Err("invalid state constant"),
        })
    }
}

impl FromStr for AdsState {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match &*s.to_ascii_lowercase() {
            "invalid" => Self::Invalid,
            "idle" => Self::Idle,
            "reset" => Self::Reset,
            "init" => Self::Init,
            "start" => Self::Start,
            "run" => Self::Run,
            "stop" => Self::Stop,
            "savecfg" => Self::SaveCfg,
            "loadcfg" => Self::LoadCfg,
            "powerfail" => Self::PowerFail,
            "powergood" => Self::PowerGood,
            "error" => Self::Error,
            "shutdown" => Self::Shutdown,
            "suspend" => Self::Suspend,
            "resume" => Self::Resume,
            "config" => Self::Config,
            "reconfig" => Self::Reconfig,
            "stopping" => Self::Stopping,
            "incompatible" => Self::Incompatible,
            "exception" => Self::Exception,
            _ => return Err("invalid state name"),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AdsStateInfo {
    pub ads_state: AdsState,
    pub device_state: u16,
}

// Structures used in communication, not exposed to user,
// but pub(crate) for the test suite.

#[derive(FromBytes, IntoBytes, Immutable, Debug, Clone)]
#[repr(C)]
pub(crate) struct AdsHeader {
    /// 0x0 - ADS command
    /// 0x1 - close port
    /// 0x1000 - open port
    /// 0x1001 - note from router (router state changed)
    /// 0x1002 - get local netid
    pub ams_cmd: u16,
    pub length: U32,
    pub dest_netid: AmsNetId,
    pub dest_port: U16,
    pub src_netid: AmsNetId,
    pub src_port: U16,
    pub command: U16,
    /// 0x01 - response
    /// 0x02 - no return
    /// 0x04 - ADS command
    /// 0x08 - system command
    /// 0x10 - high priority
    /// 0x20 - with time stamp (8 bytes added)
    /// 0x40 - UDP
    /// 0x80 - command during init phase
    /// 0x8000 - broadcast
    pub state_flags: U16,
    pub data_length: U32,
    pub error_code: U32,
    pub invoke_id: U32,
}

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub(crate) struct DeviceInfoRaw {
    pub major: u8,
    pub minor: u8,
    pub version: U16,
    pub name: [u8; 16],
}

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub(crate) struct IndexLength {
    pub index_group: U32,
    pub index_offset: U32,
    pub length: U32,
}

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub(crate) struct IndexLengthRW {
    pub index_group: U32,
    pub index_offset: U32,
    pub read_length: U32,
    pub write_length: U32,
}

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub(crate) struct ReadState {
    pub ads_state: U16,
    pub dev_state: U16,
}

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub(crate) struct WriteControl {
    pub ads_state: U16,
    pub dev_state: U16,
    pub data_length: U32,
}

#[derive(FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub(crate) struct AddNotif {
    pub index_group: U32,
    pub index_offset: U32,
    pub length: U32,
    pub trans_mode: U32,
    pub max_delay: U32,
    pub cycle_time: U32,
    pub reserved: [u8; 16],
}
