use std::{
    collections::BTreeMap,
    net::{Ipv4Addr, Shutdown},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use byteorder::{ByteOrder, LE};
use bytes::{Bytes, BytesMut};
use itertools::Itertools;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        TcpStream, ToSocketAddrs,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{Mutex, mpsc, oneshot},
    task::JoinHandle,
};
use zerocopy::{
    FromBytes, FromZeros, Immutable, IntoBytes,
    little_endian::{U16, U32},
};

use crate::{
    AdsState, AmsAddr, AmsNetId, Error, Result,
    errors::{ErrContext, ads_error},
    notif,
    protocol::{
        ADS_HEADER_SIZE, AMS_HEADER_SIZE, AddNotif, AdsHeader, Command, IndexLength, IndexLengthRW,
        ReadState, WriteControl,
    },
};

#[derive(Debug, Clone)]
pub struct ClientBuilder<RouterAddr> {
    router: RouterAddr,
    target: AmsAddr,
    source: Option<AmsAddr>,
    timeouts: Timeouts,
}

impl ClientBuilder<()> {
    pub fn new(target: AmsAddr) -> ClientBuilder<(Ipv4Addr, u16)> {
        ClientBuilder {
            router: (Ipv4Addr::new(127, 0, 0, 1), 48898),
            target,
            source: Default::default(),
            timeouts: Default::default(),
        }
    }
}

impl<RouterAddr> ClientBuilder<RouterAddr> {
    pub fn router<NextRouterAddr: ToSocketAddrs>(
        self,
        router: NextRouterAddr,
    ) -> ClientBuilder<NextRouterAddr> {
        ClientBuilder {
            router,
            target: self.target,
            source: self.source,
            timeouts: self.timeouts,
        }
    }
}

impl<RouterAddr> ClientBuilder<RouterAddr> {
    pub fn source(mut self, source: AmsAddr) -> Self {
        self.source = Some(source);
        self
    }

    pub fn timeouts(mut self, timeouts: Timeouts) -> Self {
        self.timeouts = timeouts;
        self
    }
}

impl<RouterAddr: ToSocketAddrs> ClientBuilder<RouterAddr> {
    pub async fn build(self) -> Result<Client> {
        Client::new(self.router, self.target, self.source, self.timeouts).await
    }
}

/// Holds the different timeouts that will be used by the Client.
/// None means no timeout in every case.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timeouts {
    /// Connect timeout
    pub connect: Option<Duration>,
    /// Reply read timeout
    pub read: Option<Duration>,
    /// Socket write timoeut
    pub write: Option<Duration>,
}

impl Timeouts {
    /// Create a new `Timeouts` where all values are identical.
    pub fn new(duration: Duration) -> Self {
        Self {
            connect: Some(duration),
            read: Some(duration),
            write: Some(duration),
        }
    }

    /// Create a new `Timeouts` without any timeouts specified.
    pub fn none() -> Self {
        Self {
            connect: None,
            read: None,
            write: None,
        }
    }
}

type PendingCommands = Arc<Mutex<BTreeMap<u32, oneshot::Sender<Result<(AdsHeader, Bytes)>>>>>;
type NotificationSubscribers = Arc<Mutex<BTreeMap<u32, mpsc::Sender<Bytes>>>>;

/// Represents a connection to a ADS server.
///
/// The Client's communication methods use `&self`, so that it can be freely
/// shared within one thread, or sent, between threads.
#[derive(Debug)]
pub struct Client {
    /// The AMS address of the source
    source: AmsAddr,
    /// The AMS address of the target
    target: AmsAddr,
    /// The timeouts used by the client
    timeouts: Timeouts,

    /// If we opened our local port with the router
    source_port_opened: bool,

    /// TCP connection (duplicated with the reader)
    socket_writer: Mutex<OwnedWriteHalf>,

    /// Current invoke ID (identifies the request/reply pair), incremented
    /// after each request
    invoke_id: AtomicU32,

    /// Active requests
    commands: PendingCommands,
    /// Subscriptions to notifications
    subscribers: NotificationSubscribers,

    /// IO receiver
    receiver: ClientReceiver,
}

impl Drop for Client {
    fn drop(&mut self) {
        // TODO: Delete all active notifications
        // TODO: Remove our port from the router, if necessary
        // TODO: Shutdown the socket
        // TODO: Stop the receiver
    }
}

impl Client {
    /// Open a new connection to an ADS server.
    ///
    /// If connecting to a server that has an AMS router, it needs to have a
    /// route set for the source IP and NetID, otherwise the connection will be
    /// closed immediately.  The route can be added from TwinCAT, or this
    /// crate's `udp::add_route` helper can be used to add a route via UDP
    /// message.
    ///
    /// `source` is the AMS address to to use as the source; the NetID needs to
    /// match the route entry in the server.  If `None`, the NetID is
    /// constructed from the local IP address with .1.1 appended; if there is no
    /// IPv4 address, `127.0.0.1.1.1` is used.
    ///
    /// The AMS port of `source` is not important, as long as it is not a
    /// well-known service port; an ephemeral port number > 49152 is
    /// recommended.
    ///
    /// Since all communications is supposed to be handled by an ADS router,
    /// only one TCP/ADS connection can exist between two hosts. Non-TwinCAT
    /// clients should make sure to replicate this behavior, as opening a second
    /// connection will close the first.
    pub async fn new(
        router_addr: impl ToSocketAddrs,
        target: AmsAddr,
        source: Option<AmsAddr>,
        timeouts: Timeouts,
    ) -> Result<Self> {
        let mut socket = if let Some(timeout) = timeouts.connect {
            tokio::time::timeout(timeout, TcpStream::connect(router_addr))
                .await
                .ctx("establishing connection to remote ADS router (with timeout)")?
        } else {
            TcpStream::connect(router_addr)
                .await
                .ctx("establishing connection to remote ADS router")?
        };

        // Disable Nagle to ensure small requests are sent promptly; we're
        // playing ping-pong with request reply, so no pipelining.
        socket
            .set_nodelay(true)
            .ctx("setting client socket NODELAY")?;

        // Determine our source AMS address.  If it's not specified, try to use
        // the socket's local IPv4 address, if it's IPv6 (not sure if Beckhoff
        // devices support that) use `127.0.0.1` as the last resort.
        //
        // If source is Request, send an AMS port open message to the connected
        // router to get our source address.  This is required when connecting
        // via localhost, apparently.
        let mut source_port_opened = false;
        let source = match source {
            Some(addr) => addr,
            None => {
                let request_port_msg = [0, 16, 2, 0, 0, 0, 0, 0];
                let mut reply = [0; 14];
                if let Some(timeout) = timeouts.write {
                    tokio::time::timeout(timeout, socket.write_all(&request_port_msg))
                        .await
                        .ctx("requesting port from router (with timeout)")?;
                } else {
                    socket
                        .write_all(&request_port_msg)
                        .await
                        .ctx("requesting port from router")?;
                }

                if let Some(timeout) = timeouts.read {
                    tokio::time::timeout(timeout, socket.read(&mut reply))
                        .await
                        .ctx("requesting port from router (with timeout)")?;
                } else {
                    socket
                        .write_all(&mut reply)
                        .await
                        .ctx("requesting port from router")?;
                }

                if reply[..6] != [0, 16, 8, 0, 0, 0] {
                    return Err(Error::Reply(
                        "requesting port",
                        "unexpected reply header",
                        0,
                    ));
                }
                source_port_opened = true;
                AmsAddr::new(
                    AmsNetId::from_slice(&reply[6..12]).expect("size"),
                    LE::read_u16(&reply[12..14]),
                )
            }
        };

        let (socket_reader, socket_writer) = socket.into_split();

        let commands = Arc::new(Mutex::new(BTreeMap::new()));
        let subscribers = Arc::new(Mutex::new(BTreeMap::new()));

        // Start the reader thread.
        let mut receiver = ClientReceiver::default();

        receiver.start(socket_reader, source, commands.clone(), subscribers.clone());

        Ok(Client {
            source,
            target,
            timeouts,
            source_port_opened,
            socket_writer: Mutex::new(socket_writer),
            invoke_id: AtomicU32::new(1),
            commands,
            subscribers,
            receiver,
        })
    }

    /// Read some data at a given index group/offset.  Returned data can be shorter than
    /// the buffer, the length is the return value.
    pub async fn read(
        &self,
        index_group: u32,
        index_offset: u32,
        data: &mut [u8],
    ) -> Result<usize> {
        let header = IndexLength {
            index_group: U32::new(index_group),
            index_offset: U32::new(index_offset),
            length: U32::new(data.len().try_into()?),
        };

        let mut read_len = U32::new(0);

        self.communicate(
            Command::Read,
            &[header.as_bytes()],
            &mut [read_len.as_mut_bytes(), data],
        )
        .await?;

        Ok(read_len.get() as usize)
    }

    /// Read some data at a given index group/offset, ensuring that the returned data has
    /// exactly the size of the passed buffer.
    pub async fn read_exact(
        &self,
        index_group: u32,
        index_offset: u32,
        data: &mut [u8],
    ) -> Result<()> {
        let len = self.read(index_group, index_offset, data).await?;
        if len != data.len() {
            return Err(Error::Reply(
                "read data",
                "got less data than expected",
                len as u32,
            ));
        }
        Ok(())
    }

    /// Read data of given type.
    ///
    /// Any type that supports `zerocopy::FromBytes` can be read.  You can also
    /// derive that trait on your own structures and read structured data
    /// directly from the symbol.
    ///
    /// Note: to be independent of the host's byte order, use the integer types
    /// defined in `zerocopy::byteorder`.
    pub async fn read_value<T: Default + IntoBytes + FromBytes>(
        &self,
        index_group: u32,
        index_offset: u32,
    ) -> Result<T> {
        let mut buf = T::default();
        self.read_exact(index_group, index_offset, buf.as_mut_bytes())
            .await?;
        Ok(buf)
    }

    /// Write some data to a given index group/offset.
    pub async fn write(&self, index_group: u32, index_offset: u32, data: &[u8]) -> Result<()> {
        let header = IndexLength {
            index_group: U32::new(index_group),
            index_offset: U32::new(index_offset),
            length: U32::new(data.len().try_into()?),
        };
        self.communicate(Command::Write, &[header.as_bytes(), data], &mut [])
            .await?;
        Ok(())
    }

    /// Write data of given type.
    ///
    /// See `read_value` for details.
    pub async fn write_value<T: IntoBytes + Immutable>(
        &self,
        index_group: u32,
        index_offset: u32,
        value: &T,
    ) -> Result<()> {
        self.write(index_group, index_offset, value.as_bytes())
            .await
    }

    /// Write some data to a given index group/offset and then read back some
    /// reply from there.  This is not the same as a write() followed by read();
    /// it is used as a kind of RPC call.
    pub async fn write_read(
        &self,
        index_group: u32,
        index_offset: u32,
        write_data: &[u8],
        read_data: &mut [u8],
    ) -> Result<usize> {
        let header = IndexLengthRW {
            index_group: U32::new(index_group),
            index_offset: U32::new(index_offset),
            read_length: U32::new(read_data.len().try_into()?),
            write_length: U32::new(write_data.len().try_into()?),
        };
        let mut read_len = U32::new(0);
        self.communicate(
            Command::ReadWrite,
            &[header.as_bytes(), write_data],
            &mut [read_len.as_mut_bytes(), read_data],
        )
        .await?;
        Ok(read_len.get() as usize)
    }

    /// Like `write_read`, but ensure the returned data length matches the output buffer.
    pub async fn write_read_exact(
        &self,
        index_group: u32,
        index_offset: u32,
        write_data: &[u8],
        read_data: &mut [u8],
    ) -> Result<()> {
        let len = self
            .write_read(index_group, index_offset, write_data, read_data)
            .await?;
        if len != read_data.len() {
            return Err(Error::Reply(
                "write/read data",
                "got less data than expected",
                len as u32,
            ));
        }
        Ok(())
    }

    /// Return the ADS and device state of the device.
    pub async fn get_state(&self) -> Result<(AdsState, u16)> {
        let mut state = ReadState::new_zeroed();
        self.communicate(Command::ReadState, &[], &mut [state.as_mut_bytes()])
            .await?;

        // Convert ADS state to the enum type
        let ads_state = AdsState::try_from(state.ads_state.get())
            .map_err(|e| Error::Reply("read state", e, state.ads_state.get().into()))?;

        Ok((ads_state, state.dev_state.get()))
    }

    /// (Try to) set the ADS and device state of the device.
    pub async fn write_control(&self, ads_state: AdsState, dev_state: u16) -> Result<()> {
        let data = WriteControl {
            ads_state: U16::new(ads_state as _),
            dev_state: U16::new(dev_state),
            data_length: U32::new(0),
        };
        self.communicate(Command::WriteControl, &[data.as_bytes()], &mut [])
            .await?;
        Ok(())
    }

    /// Low-level function to execute an ADS command.
    ///
    /// Writes a data from a number of input buffers, and returns data in a
    /// number of output buffers.  The latter might not be filled completely;
    /// the return value specifies the number of total valid bytes.  It is up to
    /// the caller to determine what this means in terms of the passed buffers.
    pub async fn communicate(
        &self,
        cmd: Command,
        payload_bufs: &[&[u8]],
        result_bufs: &mut [&mut [u8]],
    ) -> Result<usize> {
        // Increase the invoke ID.  We could also generate a random u32, but
        // this way the sequence of packets can be tracked.
        let dispatched_invoke_id = self.invoke_id.fetch_add(1, Ordering::Relaxed);

        // The data we send is the sum of all data_in buffers.
        let payload_len = payload_bufs.iter().map(|v| v.len()).sum::<usize>();

        // Create outgoing header.
        let ads_data_len = AMS_HEADER_SIZE + payload_len;
        let header = AdsHeader {
            ams_cmd: 0, // send command
            length: U32::new(ads_data_len.try_into()?),
            dest_netid: self.target.netid(),
            dest_port: U16::new(self.target.port()),
            src_netid: self.source.netid(),
            src_port: U16::new(self.source.port()),
            command: U16::new(cmd as u16),
            state_flags: U16::new(4), // state flags (4 = send command)
            data_length: U32::new(payload_len as u32), // overflow checked above
            error_code: U32::new(0),
            invoke_id: U32::new(dispatched_invoke_id),
        };

        let mut request_buf = Vec::with_capacity(header.length.get() as usize + payload_len);

        request_buf.extend_from_slice(header.as_bytes());

        // Collect the outgoing data.  Note, allocating a Vec and calling
        // `socket.write_all` only once is faster than writing in multiple
        // steps, even with TCP_NODELAY.
        for buf in payload_bufs.iter() {
            request_buf.extend_from_slice(buf);
        }

        let (resp_tx, resp_rx) = oneshot::channel::<Result<(AdsHeader, Bytes)>>();

        self.insert_pending_command(dispatched_invoke_id, resp_tx);

        {
            let mut writer = self.socket_writer.lock().await;

            if let Some(timeout) = self.timeouts.write {
                tokio::time::timeout(timeout, writer.write_all(&request_buf))
                    .await
                    .ctx("dispatching assembled command payload (with timeout)")?
            } else {
                writer
                    .write_all(&request_buf)
                    .await
                    .ctx("dispatching assembled command payload")?;
            }
        }

        let (resp_header, resp_buf) = match self.timeouts.read {
            Some(timeout) => match tokio::time::timeout(timeout, resp_rx).await {
                Ok(Ok(Ok((header, payload)))) => (header, payload),

                Ok(Ok(Err(e))) => {
                    self.discard_pending_command(&dispatched_invoke_id);
                    return Err(e);
                }

                Ok(Err(_recv_error)) => {
                    self.discard_pending_command(&dispatched_invoke_id);
                    return Err(Error::IoSync(
                        "waiting for response to dispatched request",
                        "response channel was closed",
                        dispatched_invoke_id,
                    ));
                }

                Err(_elapsed) => {
                    self.discard_pending_command(&dispatched_invoke_id);
                    return Err(Error::Io(
                        "waiting for response to dispatched request",
                        std::io::ErrorKind::TimedOut.into(),
                    ));
                }
            },

            None => match resp_rx.await {
                Ok(Ok((header, payload))) => (header, payload),

                Ok(Err(e)) => {
                    self.discard_pending_command(&dispatched_invoke_id);
                    return Err(e);
                }

                Err(_) => {
                    self.discard_pending_command(&dispatched_invoke_id);
                    return Err(Error::IoSync(
                        "waiting for response to dispatched request",
                        "response channel was closed",
                        dispatched_invoke_id,
                    ));
                }
            },
        };

        // Validate the incoming reply. The reader thread already made sure that
        // it is consistent and addressed to us.

        // The source netid/port must match what we sent.
        if (resp_header.src_netid, resp_header.src_port.get())
            != (self.target.netid(), self.target.port())
        {
            return Err(Error::Reply(
                cmd.action(),
                "response wasn't from commanded target",
                0,
            ));
        }

        // Command must match.
        if resp_header.command != cmd as u16 {
            return Err(Error::Reply(
                cmd.action(),
                "unexpected command",
                resp_header.command.into(),
            ));
        }

        // State flags must be "4 | 1".
        if resp_header.state_flags != 5 {
            return Err(Error::Reply(
                cmd.action(),
                "unexpected state flags",
                resp_header.state_flags.into(),
            ));
        }

        // Invoke ID must match what we sent.
        if resp_header.invoke_id != dispatched_invoke_id {
            return Err(Error::Reply(
                cmd.action(),
                "unexpected invoke ID",
                resp_header.invoke_id.get(),
            ));
        }

        // Check error code in AMS header.
        if resp_header.error_code != 0 {
            return ads_error(cmd.action(), resp_header.error_code.get());
        }

        let result = LE::read_u32(&resp_buf[..4]);

        // Check result field in payload, only relevant if error_code == 0.
        if result != 0 {
            return ads_error(cmd.action(), result);
        }

        // If we don't want return data, we're done.
        if result_bufs.is_empty() {
            return Ok(0);
        }

        // Check returned length, it needs to fill at least the first data_out
        // buffer. This also ensures that we had a result field.
        if resp_buf.len() < result_bufs[0].len() + 4 {
            return Err(Error::Reply(
                cmd.action(),
                "got less data than expected",
                resp_buf.len() as u32,
            ));
        }

        let resp_buf = &resp_buf[4..];

        // Distribute the data into the user output buffers, up to the returned
        // data length.
        let mut taken = 0;
        let mut rest_len = resp_buf.len();
        for buf in result_bufs {
            let n = buf.len().min(rest_len);
            let b = &resp_buf[taken..][..n];
            buf[..n].copy_from_slice(b);
            taken += n;
            rest_len -= n;
            if rest_len == 0 {
                break;
            }
        }

        // Return either the error or the length of data.
        Ok(resp_buf.len())
    }

    async fn insert_pending_command(
        &self,
        id: u32,
        tx: oneshot::Sender<Result<(AdsHeader, Bytes)>>,
    ) {
        self.commands.lock().await.insert(id, tx);
    }

    async fn discard_pending_command(&self, id: &u32) {
        self.commands.lock().await.remove_entry(id);
    }

    /// Add a notification handle for some index group/offset.
    ///
    /// Notifications are delivered via a MPMC channel whose reading end can be
    /// obtained from `get_notification_channel` on the `Client` object.
    /// The returned `Handle` can be used to check which notification has fired.
    ///
    /// If the notification is not deleted explictly using `delete_notification`
    /// and the `Handle`, it is deleted when the `Client` object is dropped.
    pub async fn add_notification(
        &self,
        index_group: u32,
        index_offset: u32,
        attributes: &notif::Attributes,
    ) -> Result<(notif::Handle, mpsc::Receiver<Bytes>)> {
        let data = AddNotif {
            index_group: U32::new(index_group),
            index_offset: U32::new(index_offset),
            length: U32::new(attributes.length.try_into()?),
            trans_mode: U32::new(attributes.trans_mode as u32),
            max_delay: U32::new(attributes.max_delay.as_millis().try_into()?),
            cycle_time: U32::new(attributes.cycle_time.as_millis().try_into()?),
            reserved: [0; 16],
        };
        let mut handle = U32::new(0);
        self.communicate(
            Command::AddNotification,
            &[data.as_bytes()],
            &mut [handle.as_mut_bytes()],
        )
        .await?;

        // u32, mpsc::Sender<Bytes>>>
        let (sender, receiver) = mpsc::channel(32);
        {
            let mut subscribers = self.subscribers.lock().await;
            subscribers.insert(handle.get(), sender);
        }

        Ok((handle.get(), receiver))
    }

    /// Delete a notification with given handle.
    pub async fn delete_notification(&self, handle: notif::Handle) -> Result<()> {
        self.communicate(
            Command::DeleteNotification,
            &[U32::new(handle).as_bytes()],
            &mut [],
        )
        .await?;

        {
            let mut subscribers = self.subscribers.lock().await;
            subscribers.remove(&handle);
        }

        Ok(())
    }
}

// Implementation detail: reader thread that takes replies and notifications
// and distributes them accordingly.
#[derive(Debug, Default)]
struct ClientReceiver {
    handle: Option<JoinHandle<Result<()>>>,
}

impl ClientReceiver {
    fn start(
        &mut self,
        mut socket: OwnedReadHalf,
        source: AmsAddr,
        commands: PendingCommands,
        subscribers: NotificationSubscribers,
    ) {
        let rx_worker = tokio::spawn(async move {
            let result =
                Self::reader_work(&mut socket, source, commands.clone(), subscribers.clone()).await;

            // TODO
            // let _ = socket.shutdown(Shutdown::Both);

            let mut commands = commands.lock().await;
            let keys = commands.keys().cloned().collect_vec();
            for key in keys {
                if let Some(channel) = commands.remove(&key) {
                    let err = if let Err(e) = &result {
                        Err(e.clone())
                    } else {
                        Err(Error::Reply(
                            "handling clean shutdown",
                            "pending request at client shutdown",
                            0,
                        ))
                    };

                    let _ = channel.send(err);
                };
            }

            result
        });

        let _ = self.handle.insert(rx_worker);
    }

    async fn stop(&mut self) -> Option<Result<()>> {
        self.handle.take()?.await.ok()
    }

    async fn reader_work(
        socket_rx: &mut OwnedReadHalf,
        source: AmsAddr,
        commands: PendingCommands,
        subscribers: NotificationSubscribers,
    ) -> Result<()> {
        loop {
            let mut ads_header_buf = [0u8; ADS_HEADER_SIZE];

            // TODO timeout
            socket_rx
                .read_exact(&mut ads_header_buf[..6])
                .await
                .ctx("receiving AMS/TCP header")?;

            let packet_len = LE::read_u32(&ads_header_buf[2..6]);

            let ads_header = match packet_len {
                0..=31 => {
                    let mut discard = [0u8; 31];

                    // TODO timeout
                    socket_rx
                        .read_exact(&mut discard[..packet_len as usize])
                        .await
                        .ctx("discarding bad data")?;

                    continue;
                }

                _ => {
                    // TODO timeout
                    socket_rx
                        .read_exact(&mut ads_header_buf[6..])
                        .await
                        .ctx("receiving AMS header")?;

                    AdsHeader::read_from_bytes(&ads_header_buf[..ADS_HEADER_SIZE])
                        .map_err(|_| std::io::ErrorKind::InvalidData.into())
                        .ctx("decoding AMS header")?
                }
            };

            let payload_len = ads_header.data_length.get();

            let mut payload_buf = BytesMut::zeroed(payload_len as usize);

            // TODO timeout
            socket_rx
                .read_exact(&mut payload_buf)
                .await
                .ctx("receiving Ads data payload")?;

            // Reserved bytes should be well-known
            // Anything else might be invalid data
            match LE::read_u16(ads_header_buf.as_slice()) {
                0 => (),
                1 | 4096..=4098 => continue,
                unknown => {
                    return Err(Error::Reply(
                        "interpreting received AMS packet",
                        "invalid packet",
                        unknown as _,
                    ));
                }
            }

            // If the header length fields aren't self-consistent, abort the connection.
            if payload_len != packet_len - AMS_HEADER_SIZE as u32 {
                return Err(Error::Reply(
                    "interpreting received AMS packet",
                    "AMS/TCP header and AMS header contain inconsistent data",
                    0,
                ));
            }

            // Check that the packet is meant for us.
            if (ads_header.dest_netid, ads_header.dest_port.get())
                != (source.netid(), source.port())
            {
                continue;
            }

            let invoke_id = ads_header.invoke_id.get();

            // If it looks like a reply, send it back to the requesting thread,
            // it will handle further validation.
            if ads_header.command != Command::Notification as u16 {
                match commands.lock().await.remove_entry(&invoke_id) {
                    Some((_, tx)) => {
                        if tx.send(Ok((ads_header, payload_buf.freeze()))).is_err() {
                            return Err(Error::IoSync(
                                "settling pending request",
                                "channel closed, couldn't dispatch response",
                                invoke_id,
                            ));
                        }
                    }

                    _ => {
                        return Err(Error::Reply(
                            "settling pending request",
                            "invalid invoke id received from server, aborting connection",
                            invoke_id,
                        ));
                    }
                };
            } else {
                let notif_payload_len = LE::read_u32(&payload_buf);
                if ads_header.state_flags != 4
                    || ads_header.error_code != 0
                    || notif_payload_len != payload_len - 4
                    || notif_payload_len < 4
                {
                    continue;
                }

                // Send the notification to whoever wants to receive it.
                if let Ok(notif) =
                    notif::Notification::new([ads_header_buf.as_slice(), &payload_buf].concat())
                {
                    let subscribers = subscribers.lock().await;
                    for sample in notif.samples() {
                        if let Some(subscriber) = subscribers.get(&sample.handle) {
                            subscriber.send(Bytes::copy_from_slice(sample.data));
                        }
                    }
                }
            }
        }
    }
}

impl Drop for ClientReceiver {
    fn drop(&mut self) {
        self.stop();
    }
}
