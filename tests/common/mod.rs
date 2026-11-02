//! A scriptable fake PLC that speaks just enough ADS over TCP for the tests.

#![allow(dead_code)]

use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use openstar_ads::{Ads, AdsBuilder, Timeouts};
use socket2::{Domain, Socket, Type};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    sync::{mpsc, watch, Notify},
};

const AMS_TCP_HEADER: usize = 6;
const AMS_HEADER: usize = 32;

const CMD_READ: u16 = 2;
const CMD_WRITE: u16 = 3;
const CMD_READ_STATE: u16 = 4;
const CMD_ADD_NOTIFICATION: u16 = 6;
const CMD_DELETE_NOTIFICATION: u16 = 7;
const CMD_NOTIFICATION: u16 = 8;
const CMD_READ_WRITE: u16 = 9;

const GET_SYMHANDLE_BYNAME: u32 = 0xF003;
const RW_SYMVAL_BYHANDLE: u32 = 0xF005;

const ADS_STATE_RUN: u16 = 5;
const ERR_SERVICE_NOT_SUPPORTED: u32 = 0x701;
const ERR_SYMBOL_NOT_FOUND: u32 = 0x710;

/// The value every symbol reads as.
pub const SYMBOL_VALUE: u32 = 42;

pub const SYMBOL: &str = "MAIN.example.value";

pub struct FakePlc {
    listener: TcpListener,
    sessions: u32,
}

impl FakePlc {
    pub async fn start() -> Self {
        // A small receive buffer, so a client that writes faster than the
        // PLC reads is blocked after kilobytes rather than megabytes.
        let socket = Socket::new(Domain::IPV4, Type::STREAM, None).unwrap();
        socket.set_recv_buffer_size(4096).unwrap();
        socket.set_nonblocking(true).unwrap();
        socket
            .bind(&"127.0.0.1:0".parse::<SocketAddr>().unwrap().into())
            .unwrap();
        socket.listen(8).unwrap();
        let listener = TcpListener::from_std(socket.into()).unwrap();
        Self {
            listener,
            sessions: 0,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.listener.local_addr().unwrap()
    }

    /// Accept the next client connection and serve it in the background.
    pub async fn accept(&mut self) -> Session {
        let (socket, _) = self.listener.accept().await.unwrap();
        self.sessions += 1;
        Session::serve(socket, self.sessions)
    }

    /// An [`Ads`] pointed at this PLC with the given timeout everywhere.
    pub async fn ads(&self, timeout: Duration) -> Ads {
        self.ads_builder()
            .timeouts(Timeouts::new(timeout))
            .build()
            .await
            .unwrap()
    }

    /// An [`Ads`] pointed at this PLC with no timeouts at all.
    pub async fn ads_without_timeouts(&self) -> Ads {
        self.ads_builder()
            .timeouts(Timeouts::none())
            .build()
            .await
            .unwrap()
    }

    /// An [`AdsBuilder`] pointed at this PLC.
    fn ads_builder(&self) -> AdsBuilder<SocketAddr> {
        AdsBuilder::new("1.2.3.4.1.1:851".parse().unwrap())
            .router(self.addr())
            .source("5.6.7.8.1.1:30000".parse().unwrap())
    }
}

enum Control {
    HoldReplies,
    ReleaseReplies,
    Notify(Vec<u8>),
    Close,
    KeepOpenAfterClientShutdown,
    SendMalformedFrame,
    StopReading,
}

/// One client connection to the fake PLC.
pub struct Session {
    control: mpsc::UnboundedSender<Control>,
    requests: Arc<AtomicUsize>,
    request_seen: Arc<Notify>,
}

impl Session {
    fn serve(socket: TcpStream, number: u32) -> Self {
        let (reader, writer) = socket.into_split();
        let (control, control_rx) = mpsc::unbounded_channel();
        let requests = Arc::new(AtomicUsize::new(0));
        let request_seen = Arc::new(Notify::new());

        let worker = Worker {
            writer,
            control_rx,
            requests: requests.clone(),
            request_seen: request_seen.clone(),
            // Per-session handles, so a stale handle from an earlier connection is rejected.
            next_symbol_handle: number * 1000,
            next_notification_handle: number * 1000,
            notification_handles: Vec::new(),
            client: None,
            holding: false,
            held: Vec::new(),
            keep_open_after_client_shutdown: false,
            reading: watch::Sender::new(true),
        };
        tokio::spawn(worker.run(reader));

        Self {
            control,
            requests,
            request_seen,
        }
    }

    /// Queue replies instead of sending them, until [`release_replies`].
    pub fn hold_replies(&self) {
        let _ = self.control.send(Control::HoldReplies);
    }

    /// Send every held reply and go back to replying at once.
    pub fn release_replies(&self) {
        let _ = self.control.send(Control::ReleaseReplies);
    }

    /// Send one notification carrying `data` for every active subscription.
    pub fn notify(&self, data: &[u8]) {
        let _ = self.control.send(Control::Notify(data.to_vec()));
    }

    /// Close the TCP connection.
    pub fn close(&self) {
        let _ = self.control.send(Control::Close);
    }

    /// Send a frame the client cannot make sense of, leaving the socket open.
    pub fn send_malformed_frame(&self) {
        let _ = self.control.send(Control::SendMalformedFrame);
    }

    /// Stop reading from the socket, so the client's writes back up.
    pub fn stop_reading(&self) {
        let _ = self.control.send(Control::StopReading);
    }

    /// Keep our side of the socket open after the client shuts down its
    /// write half, as a peer that never closes would.
    pub fn keep_open_after_client_shutdown(&self) {
        let _ = self.control.send(Control::KeepOpenAfterClientShutdown);
    }

    /// Wait until at least `count` requests have arrived in this session.
    pub async fn wait_for_requests(&self, count: usize) {
        loop {
            let notified = self.request_seen.notified();
            if self.requests.load(Ordering::SeqCst) >= count {
                return;
            }
            notified.await;
        }
    }
}

struct Worker {
    writer: OwnedWriteHalf,
    control_rx: mpsc::UnboundedReceiver<Control>,
    requests: Arc<AtomicUsize>,
    request_seen: Arc<Notify>,
    next_symbol_handle: u32,
    next_notification_handle: u32,
    notification_handles: Vec<u32>,
    client: Option<AmsHeader>,
    holding: bool,
    held: Vec<Vec<u8>>,
    keep_open_after_client_shutdown: bool,
    reading: watch::Sender<bool>,
}

impl Worker {
    async fn run(mut self, reader: OwnedReadHalf) {
        let (frames_tx, mut frames) = mpsc::unbounded_channel();
        tokio::spawn(read_frames(reader, frames_tx, self.reading.subscribe()));
        let mut client_gone = false;

        loop {
            tokio::select! {
                frame = frames.recv(), if !client_gone => match frame {
                    Some(frame) => {
                        if !self.handle_request(frame).await {
                            return;
                        }
                    }
                    None => {
                        client_gone = true;
                        let stopped_reading = !*self.reading.borrow();
                        if !self.keep_open_after_client_shutdown && !stopped_reading {
                            return;
                        }
                    }
                },
                control = self.control_rx.recv() => match control {
                    Some(Control::HoldReplies) => self.holding = true,
                    Some(Control::ReleaseReplies) => {
                        self.holding = false;
                        for reply in std::mem::take(&mut self.held) {
                            if self.writer.write_all(&reply).await.is_err() {
                                return;
                            }
                        }
                    }
                    Some(Control::Notify(data)) => {
                        if let Some(client) = &self.client {
                            let packet = notification(client, &self.notification_handles, &data);
                            if self.writer.write_all(&packet).await.is_err() {
                                return;
                            }
                        }
                    }
                    Some(Control::KeepOpenAfterClientShutdown) => {
                        self.keep_open_after_client_shutdown = true
                    }
                    Some(Control::SendMalformedFrame) => {
                        let mut packet = 0xFFFFu16.to_le_bytes().to_vec();
                        packet.extend_from_slice(&(AMS_HEADER as u32).to_le_bytes());
                        packet.resize(AMS_TCP_HEADER + AMS_HEADER, 0);
                        if self.writer.write_all(&packet).await.is_err() {
                            return;
                        }
                    }
                    Some(Control::StopReading) => {
                        self.reading.send_replace(false);
                    }
                    Some(Control::Close) | None => return,
                },
            }
        }
    }

    /// Returns false when the socket is gone.
    async fn handle_request(&mut self, frame: Vec<u8>) -> bool {
        let header = AmsHeader::parse(&frame[AMS_TCP_HEADER..]);
        let data = &frame[AMS_TCP_HEADER + AMS_HEADER..];
        self.client = Some(header.clone());
        self.requests.fetch_add(1, Ordering::SeqCst);
        self.request_seen.notify_waiters();

        let (body, error_code) = match header.command {
            CMD_READ_STATE => (
                [ADS_STATE_RUN.to_le_bytes(), 0u16.to_le_bytes()].concat(),
                0,
            ),
            CMD_READ_WRITE => {
                let index_group = u32::from_le_bytes(data[0..4].try_into().unwrap());
                if index_group == GET_SYMHANDLE_BYNAME {
                    let handle = self.next_symbol_handle;
                    self.next_symbol_handle += 1;
                    (with_length(&handle.to_le_bytes()), 0)
                } else {
                    (vec![], ERR_SERVICE_NOT_SUPPORTED)
                }
            }
            CMD_READ => {
                let index_group = u32::from_le_bytes(data[0..4].try_into().unwrap());
                let handle = u32::from_le_bytes(data[4..8].try_into().unwrap());
                if index_group == RW_SYMVAL_BYHANDLE && self.is_symbol_handle(handle) {
                    (with_length(&SYMBOL_VALUE.to_le_bytes()), 0)
                } else {
                    (vec![], ERR_SYMBOL_NOT_FOUND)
                }
            }
            CMD_WRITE => (vec![], 0),
            CMD_ADD_NOTIFICATION => {
                let handle = self.next_notification_handle;
                self.next_notification_handle += 1;
                self.notification_handles.push(handle);
                (handle.to_le_bytes().to_vec(), 0)
            }
            CMD_DELETE_NOTIFICATION => {
                let handle = u32::from_le_bytes(data[0..4].try_into().unwrap());
                self.notification_handles.retain(|h| *h != handle);
                (vec![], 0)
            }
            _ => (vec![], ERR_SERVICE_NOT_SUPPORTED),
        };

        let mut payload = error_code.to_le_bytes().to_vec();
        payload.extend_from_slice(&body);
        let reply = header.reply(&payload);

        if self.holding {
            self.held.push(reply);
            true
        } else {
            self.writer.write_all(&reply).await.is_ok()
        }
    }

    fn is_symbol_handle(&self, handle: u32) -> bool {
        let base = self.next_symbol_handle / 1000 * 1000;
        (base..self.next_symbol_handle).contains(&handle)
    }
}

fn with_length(data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(data);
    out
}

/// Reads whole AMS frames from the socket until it closes or reading is
/// stopped, which drops the read half mid-frame so the socket fills up.
async fn read_frames(
    reader: OwnedReadHalf,
    frames: mpsc::UnboundedSender<Vec<u8>>,
    mut reading: watch::Receiver<bool>,
) {
    tokio::select! {
        _ = reading.wait_for(|reading| !*reading) => {}
        _ = read_frames_until_closed(reader, frames) => {}
    }
}

async fn read_frames_until_closed(
    mut reader: OwnedReadHalf,
    frames: mpsc::UnboundedSender<Vec<u8>>,
) {
    loop {
        let mut tcp_header = [0u8; AMS_TCP_HEADER];
        if reader.read_exact(&mut tcp_header).await.is_err() {
            return;
        }
        let length = u32::from_le_bytes(tcp_header[2..6].try_into().unwrap()) as usize;
        let mut frame = tcp_header.to_vec();
        frame.resize(AMS_TCP_HEADER + length, 0);
        if reader
            .read_exact(&mut frame[AMS_TCP_HEADER..])
            .await
            .is_err()
        {
            return;
        }
        if frames.send(frame).is_err() {
            return;
        }
    }
}

#[derive(Clone)]
struct AmsHeader {
    target: [u8; 8],
    source: [u8; 8],
    command: u16,
    state_flags: u16,
    invoke_id: u32,
}

impl AmsHeader {
    fn parse(bytes: &[u8]) -> Self {
        Self {
            target: bytes[0..8].try_into().unwrap(),
            source: bytes[8..16].try_into().unwrap(),
            command: u16::from_le_bytes(bytes[16..18].try_into().unwrap()),
            state_flags: u16::from_le_bytes(bytes[18..20].try_into().unwrap()),
            invoke_id: u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
        }
    }

    /// A reply to this request carrying `payload`.
    fn reply(&self, payload: &[u8]) -> Vec<u8> {
        frame(
            &self.source,
            &self.target,
            self.command,
            self.state_flags | 1,
            self.invoke_id,
            payload,
        )
    }
}

/// A frame from the PLC to the client.
fn frame(
    target: &[u8; 8],
    source: &[u8; 8],
    command: u16,
    state_flags: u16,
    invoke_id: u32,
    payload: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(AMS_TCP_HEADER + AMS_HEADER + payload.len());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&((AMS_HEADER + payload.len()) as u32).to_le_bytes());
    out.extend_from_slice(target);
    out.extend_from_slice(source);
    out.extend_from_slice(&command.to_le_bytes());
    out.extend_from_slice(&state_flags.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&invoke_id.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// One notification packet with one stamp holding one sample per handle.
fn notification(client: &AmsHeader, handles: &[u32], data: &[u8]) -> Vec<u8> {
    let mut stamp = 0u64.to_le_bytes().to_vec();
    stamp.extend_from_slice(&(handles.len() as u32).to_le_bytes());
    for handle in handles {
        stamp.extend_from_slice(&handle.to_le_bytes());
        stamp.extend_from_slice(&(data.len() as u32).to_le_bytes());
        stamp.extend_from_slice(data);
    }

    let mut body = 1u32.to_le_bytes().to_vec();
    body.extend_from_slice(&stamp);

    let payload = with_length(&body);
    frame(
        &client.source,
        &client.target,
        CMD_NOTIFICATION,
        4,
        0,
        &payload,
    )
}
