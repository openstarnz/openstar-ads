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
    fn action(self) -> &'static str {
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
pub(crate) struct ResultLength {
    pub result: U32,
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

/// A single request for a [`Device::read_multi`] request.
pub struct ReadRequest<'buf> {
    req: IndexLength,
    res: ResultLength,
    rbuf: &'buf mut [u8],
}

impl<'buf> ReadRequest<'buf> {
    /// Create the request with given index group, index offset and result buffer.
    pub fn new(index_group: u32, index_offset: u32, buffer: &'buf mut [u8]) -> Self {
        Self {
            req: IndexLength {
                index_group: U32::new(index_group),
                index_offset: U32::new(index_offset),
                length: U32::new(buffer.len() as u32),
            },
            res: ResultLength::new_zeroed(),
            rbuf: buffer,
        }
    }

    /// Get the actual returned data.
    ///
    /// If the request returned an error, returns Err.
    pub fn data(&self) -> Result<&[u8]> {
        if self.res.result.get() != 0 {
            ads_error("multi-read data", self.res.result.get())
        } else {
            Ok(&self.rbuf[..self.res.length.get() as usize])
        }
    }
}

/// A single request for a [`Device::write_multi`] request.
pub struct WriteRequest<'buf> {
    req: IndexLength,
    res: U32,
    wbuf: &'buf [u8],
}

impl<'buf> WriteRequest<'buf> {
    /// Create the request with given index group, index offset and input buffer.
    pub fn new(index_group: u32, index_offset: u32, buffer: &'buf [u8]) -> Self {
        Self {
            req: IndexLength {
                index_group: U32::new(index_group),
                index_offset: U32::new(index_offset),
                length: U32::new(buffer.len() as u32),
            },
            res: U32::default(),
            wbuf: buffer,
        }
    }

    /// Verify that the data was successfully written.
    ///
    /// If the request returned an error, returns Err.
    pub fn ensure(&self) -> Result<()> {
        if self.res.get() != 0 {
            ads_error("multi-write data", self.res.get())
        } else {
            Ok(())
        }
    }
}

/// A single request for a [`Device::write_read_multi`] request.
pub struct WriteReadRequest<'buf> {
    req: IndexLengthRW,
    res: ResultLength,
    wbuf: &'buf [u8],
    rbuf: &'buf mut [u8],
}

impl<'buf> WriteReadRequest<'buf> {
    /// Create the request with given index group, index offset and input and
    /// result buffers.
    pub fn new(
        index_group: u32,
        index_offset: u32,
        write_buffer: &'buf [u8],
        read_buffer: &'buf mut [u8],
    ) -> Self {
        Self {
            req: IndexLengthRW {
                index_group: U32::new(index_group),
                index_offset: U32::new(index_offset),
                read_length: U32::new(read_buffer.len() as u32),
                write_length: U32::new(write_buffer.len() as u32),
            },
            res: ResultLength::new_zeroed(),
            wbuf: write_buffer,
            rbuf: read_buffer,
        }
    }

    /// Get the actual returned data.
    ///
    /// If the request returned an error, returns Err.
    pub fn data(&self) -> Result<&[u8]> {
        if self.res.result.get() != 0 {
            ads_error("multi-read/write data", self.res.result.get())
        } else {
            Ok(&self.rbuf[..self.res.length.get() as usize])
        }
    }
}

/// A single request for a [`Device::add_notification_multi`] request.
pub struct AddNotifRequest {
    req: AddNotif,
    res: ResultLength, // length is the handle
}

impl AddNotifRequest {
    /// Create the request with given index group, index offset and notification
    /// attributes.
    pub fn new(index_group: u32, index_offset: u32, attributes: &notif::Attributes) -> Self {
        Self {
            req: AddNotif {
                index_group: U32::new(index_group),
                index_offset: U32::new(index_offset),
                length: U32::new(attributes.length as u32),
                trans_mode: U32::new(attributes.trans_mode as u32),
                max_delay: U32::new(attributes.max_delay.as_millis() as u32),
                cycle_time: U32::new(attributes.cycle_time.as_millis() as u32),
                reserved: [0; 16],
            },
            res: ResultLength::new_zeroed(),
        }
    }

    /// Get the returned notification handle.
    ///
    /// If the request returned an error, returns Err.
    pub fn handle(&self) -> Result<notif::Handle> {
        if self.res.result.get() != 0 {
            ads_error("multi-read/write data", self.res.result.get())
        } else {
            Ok(self.res.length.get())
        }
    }
}

/// A single request for a [`Device::delete_notification_multi`] request.
pub struct DelNotifRequest {
    req: U32,
    res: U32,
}

impl DelNotifRequest {
    /// Create the request with given index group, index offset and notification
    /// attributes.
    pub fn new(handle: notif::Handle) -> Self {
        Self {
            req: U32::new(handle),
            res: U32::default(),
        }
    }

    /// Verify that the handle was successfully deleted.
    ///
    /// If the request returned an error, returns Err.
    pub fn ensure(&self) -> Result<()> {
        if self.res.get() != 0 {
            ads_error("multi-read/write data", self.res.get())
        } else {
            Ok(())
        }
    }
}

fn fixup_write_read_return_buffers(requests: &mut [WriteReadRequest]) {
    // Calculate the initial (using buffer sizes) and actual (using result
    // sizes) offsets of each request.
    let offsets = requests
        .iter()
        .scan((0, 0), |(init_cum, act_cum), req| {
            let (init, act) = (req.rbuf.len(), req.res.length.get() as usize);
            let current = Some((*init_cum, *act_cum, init, act));
            assert!(init >= act);
            *init_cum += init;
            *act_cum += act;
            current
        })
        .collect_vec();

    // Go through the buffers in reverse order.
    for i in (0..requests.len()).rev() {
        let (my_initial, my_actual, _, mut size) = offsets[i];
        if size == 0 {
            continue;
        }
        if my_initial == my_actual {
            // Offsets match, no further action required since all
            // previous buffers must be of full length too.
            break;
        }

        // Check in which buffer our last byte is.
        let mut j = offsets[..i + 1]
            .iter()
            .rposition(|r| r.0 < my_actual + size)
            .expect("index must be somewhere");
        let mut j_end = my_actual + size - offsets[j].0;

        // Copy the required number of bytes from every buffer from j up to i.
        loop {
            let n = j_end.min(size);
            size -= n;
            if i == j {
                requests[i].rbuf.copy_within(j_end - n..j_end, size);
            } else {
                let (first, second) = requests.split_at_mut(i);
                second[0].rbuf[size..][..n].copy_from_slice(&first[j].rbuf[j_end - n..j_end]);
            }
            if size == 0 {
                break;
            }
            j -= 1;
            j_end = offsets[j].2;
        }
    }
}

#[test]
fn test_fixup_buffers() {
    let mut buf0 = *b"12345678AB";
    let mut buf1 = *b"CDEFabc";
    let mut buf2 = *b"dxyUVW";
    let mut buf3 = *b"XYZY";
    let mut buf4 = *b"XW----";
    let mut buf5 = *b"-------------";
    let reqs = &mut [
        WriteReadRequest::new(0, 0, &[], &mut buf0),
        WriteReadRequest::new(0, 0, &[], &mut buf1),
        WriteReadRequest::new(0, 0, &[], &mut buf2),
        WriteReadRequest::new(0, 0, &[], &mut buf3),
        WriteReadRequest::new(0, 0, &[], &mut buf4),
        WriteReadRequest::new(0, 0, &[], &mut buf5),
    ];
    reqs[0].res.length.set(8);
    reqs[1].res.length.set(6);
    reqs[2].res.length.set(0);
    reqs[3].res.length.set(4);
    reqs[4].res.length.set(2);
    reqs[5].res.length.set(9);

    fixup_write_read_return_buffers(reqs);

    assert!(&reqs[5].rbuf[..9] == b"UVWXYZYXW");
    assert!(&reqs[4].rbuf[..2] == b"xy");
    assert!(&reqs[3].rbuf[..4] == b"abcd");
    assert!(&reqs[1].rbuf[..6] == b"ABCDEF");
    assert!(&reqs[0].rbuf[..8] == b"12345678");
}
