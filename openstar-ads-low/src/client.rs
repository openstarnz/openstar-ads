use std::{
    collections::BTreeMap,
    net::{Ipv4Addr, Shutdown},
    sync::{Arc, atomic::AtomicU32},
    time::Duration,
};

use byteorder::LE;
use bytes::{Bytes, BytesMut};
use tokio::{
    net::{
        TcpStream, ToSocketAddrs,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{Mutex, mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    AmsAddr, AmsNetId, Error, Result, notif,
    protocol::{ADS_HEADER_SIZE, AMS_HEADER_SIZE, AdsHeader, Command},
};

#[derive(Debug, Clone)]
pub struct ClientBuilder<RouterAddr> {
    router_addr: RouterAddr,
    dst_ams_addr: AmsAddr,
    src_ams_addr: Option<AmsAddr>,
    timeouts: Timeouts,
}

impl ClientBuilder<()> {
    pub fn new(dst_ams_addr: AmsAddr) -> ClientBuilder<(Ipv4Addr, u16)> {
        ClientBuilder {
            router_addr: (Ipv4Addr::new(127, 0, 0, 1), 48898),
            dst_ams_addr,
            src_ams_addr: Default::default(),
            timeouts: Default::default(),
        }
    }
}

impl<RouterAddr> ClientBuilder<RouterAddr> {
    pub fn router_addr<NextRouterAddr: ToSocketAddrs>(
        self,
        router_addr: NextRouterAddr,
    ) -> ClientBuilder<NextRouterAddr> {
        ClientBuilder {
            router_addr,
            dst_ams_addr: self.dst_ams_addr,
            src_ams_addr: self.src_ams_addr,
            timeouts: self.timeouts,
        }
    }
}

impl<RouterAddr> ClientBuilder<RouterAddr> {
    pub fn src_ams_addr(mut self, src_ams_addr: AmsAddr) -> Self {
        self.src_ams_addr = Some(src_ams_addr);
        self
    }

    pub fn timeouts(mut self, timeouts: Timeouts) -> Self {
        self.timeouts = timeouts;
        self
    }
}

impl<RouterAddr: ToSocketAddrs> ClientBuilder<RouterAddr> {
    pub async fn build(self) -> Result<Client> {
        Client::new(
            self.router_addr,
            self.dst_ams_addr,
            self.src_ams_addr,
            self.timeouts,
        )
        .await
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
    /// TCP connection (duplicated with the reader)
    socket_writer: OwnedWriteHalf,
    /// Current invoke ID (identifies the request/reply pair), incremented
    /// after each request
    invoke_id: AtomicU32,
    /// Read timeout (actually receive timeout for the channel)
    read_timeout: Option<Duration>,
    /// The AMS address of the client
    source: AmsAddr,
    /// Active requests
    commands: PendingCommands,
    /// Subscriptions to notifications
    subscribers: NotificationSubscribers,
    /// IO receiver
    receiver: ClientReceiver,
    /// If we opened our local port with the router
    source_port_opened: bool,
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
    /// `src_ams_addr` is the AMS address to to use as the source; the NetID needs to
    /// match the route entry in the server.  If `None`, the NetID is
    /// constructed from the local IP address with .1.1 appended; if there is no
    /// IPv4 address, `127.0.0.1.1.1` is used.
    ///
    /// The AMS port of `source` is not important, as long as it is not a
    /// well-known service port; an ephemeral port number > 49152 is
    /// recommended. The default port is set to 58913.
    ///
    /// Since all communications is supposed to be handled by an ADS router,
    /// only one TCP/ADS connection can exist between two hosts. Non-TwinCAT
    /// clients should make sure to replicate this behavior, as opening a second
    /// connection will close the first.
    pub async fn connect(
        router_addr: impl ToSocketAddrs,
        dst_ams_addr: AmsAddr,
        src_ams_addr: Option<AmsAddr>,
        timeouts: Timeouts,
    ) -> Result<Self> {
        let mut socket = if let Some(timeout) = timeouts.connect {
            tokio::time::timeout(timeout, TcpStream::connect(router_addr))
                .await
                .ctx("establishing connetion to remote ADS router (with timeout)")??
        } else {
            TcpStream::connect(router_addr).ctx("establishing connection to remote ADS router")?
        };

        let (socket_reader, socket_writer) = socket.into_split();

        // Disable Nagle to ensure small requests are sent promptly; we're
        // playing ping-pong with request reply, so no pipelining.
        socket
            .set_nodelay(true)
            .await
            .ctx("setting client socket NODELAY")?;
        socket
            .set_write_timeout(timeouts.write)
            .await
            .ctx("setting client socket write timeout")?;
        socket
            .set_read_timeout(timeouts.read)
            .await
            .ctx("setting client socket read timeout")?;

        // Determine our source AMS address.  If it's not specified, try to use
        // the socket's local IPv4 address, if it's IPv6 (not sure if Beckhoff
        // devices support that) use `127.0.0.1` as the last resort.
        //
        // If source is Request, send an AMS port open message to the connected
        // router to get our source address.  This is required when connecting
        // via localhost, apparently.
        let mut source_port_opened = false;
        let source = match src_ams_addr {
            Some(addr) => addr,
            None => {
                let request_port_msg = [0, 16, 2, 0, 0, 0, 0, 0];
                let mut reply = [0; 14];
                socket
                    .write_all(&request_port_msg)
                    .ctx("requesting port from router")
                    .await?;
                socket
                    .read_exact(&mut reply)
                    .ctx("requesting port from router")
                    .await?;
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

        let commands = Arc::new(Mutex::new(BTreeMap::new()));
        let subscribers = Arc::new(Mutex::new(BTreeMap::new()));

        // Start the reader thread.
        let mut receiver = ClientReceiver::default();

        receiver.start(socket_reader, source, commands, subscribers);

        Ok(Client {
            socket_writer,
            invoke_id: AtomicU32::new(1),
            read_timeout: timeouts.read,
            source,
            commands,
            subscribers,
            receiver,
            source_port_opened,
        })
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
        socket: OwnedReadHalf,
        source: AmsAddr,
        commands: PendingCommands,
        subscribers: NotificationSubscribers,
    ) {
        let rx_worker = tokio::spawn(async move {
            let result = Self::reader_work(
                socket.as_mut(),
                source,
                commands.clone(),
                subscribers.clone(),
            )
            .await;

            let _ = socket.shutdown(Shutdown::Both);

            if let Ok(ref mut commands) = commands.lock() {
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
            }

            result
        });

        let _ = self.handle.insert(rx_worker);
    }

    fn stop(&mut self) -> Option<Result<()>> {
        self.handle.take()?.join().ok()
    }

    async fn reader_work(
        socket_rx: &mut TcpStream,
        source: AmsAddr,
        commands: PendingCommands,
        subscribers: NotificationSubscribers,
    ) -> Result<()> {
        loop {
            let mut ads_header_buf = [0u8; ADS_HEADER_SIZE];

            socket_rx
                .read_exact(&mut ads_header_buf[..6])
                .ctx("receiving AMS/TCP header")?;

            let packet_len = LE::read_u32(&ads_header_buf[2..6]);

            let ads_header = match packet_len {
                0..=31 => {
                    let mut discard = [0u8; 31];

                    socket_rx
                        .read_exact(&mut discard[..packet_len as usize])
                        .ctx("discarding bad data")?;

                    continue;
                }

                _ => {
                    socket_rx
                        .read_exact(&mut ads_header_buf[6..])
                        .ctx("receiving AMS header")?;

                    AdsHeader::read_from_bytes(&ads_header_buf[..ADS_HEADER_SIZE])
                        .map_err(|_| std::io::ErrorKind::InvalidData.into())
                        .ctx("decoding AMS header")?
                }
            };

            let payload_len = ads_header.data_length.get();

            let mut payload_buf = BytesMut::zeroed(payload_len as usize);

            socket_rx
                .read_exact(&mut payload_buf)
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
                            subscriber.send(sample.data.into());
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
