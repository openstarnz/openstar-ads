use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{Receiver, Sender};
use std::{
    net::{SocketAddr, ToSocketAddrs},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast;

use ads::{AmsAddr, Client};

use crate::{
    data_types::{
        symbol_map::{SymbolMap, SymbolMapExt, SymbolTypeMap, SymbolTypeMapExt},
        symbol_tree::SymbolTree,
        symbol_type_tree::SymbolTypeTree,
        PlcDataType,
    },
    plc_client::PlcClient,
};

#[derive(Clone)]
pub struct PlcConnection {
    ads_router_address: SocketAddr,
    plc_ams_address: AmsAddr,
    local_ams_address: Option<AmsAddr>,
    set_to_run_mode: bool,
    state: Arc<Mutex<PlcConnectionState>>,
}

pub struct PlcConnectionBuilder {
    ads_router_address: SocketAddr,
    plc_ams_address: AmsAddr,
    local_ams_address: Option<AmsAddr>,
    set_to_run_mode: bool,
}

impl PlcConnectionBuilder {
    pub fn new(ads_router_address: impl ToSocketAddrs, plc_ams_address: AmsAddr) -> Self {
        let ads_router_address = ads_router_address
            .to_socket_addrs()
            .expect("Error: could not convert ads router address to socket address")
            .next()
            .expect("Error: could not convert ads router address to socket address");

        Self {
            ads_router_address,
            plc_ams_address,
            local_ams_address: None,
            set_to_run_mode: false,
        }
    }

    pub fn with_local_ams_address(self, local_ams_address: Option<AmsAddr>) -> Self {
        Self {
            local_ams_address,
            ..self
        }
    }

    pub fn set_to_run_mode(self, set_to_run_mode: bool) -> Self {
        Self {
            set_to_run_mode,
            ..self
        }
    }

    pub fn build(self) -> PlcConnection {
        PlcConnection {
            ads_router_address: self.ads_router_address,
            plc_ams_address: self.plc_ams_address,
            local_ams_address: self.local_ams_address,
            set_to_run_mode: self.set_to_run_mode,
            state: Default::default(),
        }
    }
}

// TODO: this is hacky, due to self_cell usage in PlcClient, but we know PlcConnection is thread safe due to Arc<Mutex<...>>
unsafe impl Send for PlcConnection {}
unsafe impl Sync for PlcConnection {}

impl PlcConnection {
    /// Blocks the current thread until a PLC is successfully connected over ADS.
    pub fn run_connection_loop(&self) {
        loop {
            {
                let mut plc_connection_state = self.state.lock().unwrap();
                if let Err(error) = plc_connection_state.connect(
                    self.ads_router_address,
                    self.plc_ams_address,
                    self.local_ams_address,
                    self.set_to_run_mode,
                ) {
                    println!("PLC connection failed, {}. Retrying in 2 seconds...", error);
                } else {
                    println!("PLC connection successful!");

                    return;
                }
            }

            std::thread::sleep(Duration::from_secs(2));
        }
    }

    pub fn disconnect(&self) {
        let mut plc_connection_state = self.state.lock().unwrap();

        plc_connection_state.disconnect();
    }

    pub fn is_connected(&self) -> bool {
        let plc_connection_state = self.state.lock().unwrap();

        match *plc_connection_state {
            PlcConnectionState::Connected(_) => true,
            PlcConnectionState::Disconnected => false,
        }
    }

    /// Gets a symbol type tree for a given symbol path.
    ///
    /// Returns None if the PLC is not connected.
    pub fn get_dynamic_type_tree(&self, name: &str) -> Result<Option<SymbolTypeTree>> {
        let mut plc_connection_state = self.state.lock().unwrap();

        if let Some(client) = plc_connection_state.client_mut() {
            let type_tree = client.get_dynamic_type_tree(name).map_err(|error| {
                println!(
                    "PLC client error when getting information for symbol {}: {}",
                    name, error
                );

                plc_connection_state.handle_disconnect_error(&error);

                error
            })?;

            return Ok(Some(type_tree));
        }

        Ok(None)
    }

    /// Gets a symbol type tree for a given symbol path.
    ///
    /// Returns None if the PLC is not connected.
    pub async fn start_dynamic_symbol_receiver(
        &self,
        name: String,
        symbol_type_tree: SymbolTypeTree,
        sender_channel: Sender<SymbolTree>,
    ) -> Result<Option<()>> {
        let mut notif_handle = None;
        {
            let mut plc_connection_state = self.state.lock().unwrap();

            if let Some(client) = plc_connection_state.client_mut() {
                notif_handle = Some(
                    client
                        .add_dynamic_symbol_notification(&name, &symbol_type_tree)
                        .map_err(|error| {
                            println!(
                                "PLC client error when getting information for symbol {}: {}",
                                name, error
                            );

                            plc_connection_state.handle_disconnect_error(&error);

                            error
                        })?,
                );
            }
        }

        let Some(notif_receiver) = self.notification_receiver() else {
            return Ok(None);
        };

        let Some(notif_handle) = notif_handle else {
            return Ok(None);
        };

        // Runs the actual notification loop which filters for the correct symbol handle.
        for notif in notif_receiver {
            for sample in notif.samples() {
                if sample.handle == notif_handle {
                    let symbol_tree: SymbolTree = (&symbol_type_tree, sample.data, 0).into();
                    // If there is an error sending it means that all receivers are gone and therefore this thread has successfully ended.
                    if let Err(_err) = sender_channel.send(symbol_tree) {
                        return Ok(Some(()));
                    };
                }
            }
        }

        Ok(Some(()))
    }

    /// Gets a symbol type tree for a given symbol path.
    ///
    /// Returns None if the PLC is not connected.
    pub async fn start_dynamic_symbol_map_receiver(
        &self,
        name: String,
        symbol_type_tree: SymbolTypeTree,
        sender_channel: broadcast::Sender<SymbolMap>,
    ) -> Result<Option<()>> {
        let mut notif_handle = None;
        {
            let mut plc_connection_state = self.state.lock().unwrap();

            if let Some(client) = plc_connection_state.client_mut() {
                notif_handle = Some(
                    client
                        .add_dynamic_symbol_notification(&name, &symbol_type_tree)
                        .map_err(|error| {
                            println!(
                                "PLC client error when getting information for symbol {}: {}",
                                name, error
                            );

                            plc_connection_state.handle_disconnect_error(&error);

                            error
                        })?,
                );
            }
        }

        let Some(notif_receiver) = self.notification_receiver() else {
            return Ok(None);
        };

        let Some(notif_handle) = notif_handle else {
            return Ok(None);
        };

        let symbol_type_map = SymbolTypeMap::from_tree(symbol_type_tree, 0, "".to_string());
        // Runs the actual notification loop which filters for the correct symbol handle.
        loop {
            let notif_result = notif_receiver.recv_timeout(std::time::Duration::from_secs(5));
            let notif = notif_result
                .context("Receive error while receiving from PLC notification channel.")?;
            for sample in notif.samples() {
                if sample.handle == notif_handle {
                    let symbol_tree: SymbolMap =
                        SymbolMap::from_bytes(&symbol_type_map, sample.data).into();
                    // If there is an error sending it means that all receivers are gone and therefore this thread has successfully ended.
                    if let Err(_err) = sender_channel.send(symbol_tree) {
                        return Ok(Some(()));
                    };
                }
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    /// Read a symbol from the PLC.
    ///
    /// Returns None if the PLC is not connected.
    pub fn read_symbol<T: PlcDataType>(&self, name: &str) -> Result<Option<T>> {
        let mut plc_connection_state = self.state.lock().unwrap();

        if let Some(client) = plc_connection_state.client_mut() {
            let value = client.read_symbol(name).map_err(|error| {
                println!("PLC client error when reading symbol {}: {}", name, error);

                plc_connection_state.handle_disconnect_error(&error);

                error
            })?;

            return Ok(Some(value));
        }

        Ok(None)
    }

    /// Calls an RPC method on the PLC that returns a value.
    ///
    /// Returns None if the PLC is not connected.
    pub fn fetch_from_rpc_method<T: PlcDataType>(&self, name: &str) -> Result<Option<T>> {
        let mut plc_connection_state = self.state.lock().unwrap();

        if let Some(client) = plc_connection_state.client_mut() {
            let value = client.fetch_from_rpc_method(name).map_err(|error| {
                eprintln!(
                    "PLC client error when invoking RPC method {}: {}",
                    name, error
                );

                plc_connection_state.handle_disconnect_error(&error);

                error
            })?;

            return Ok(Some(value));
        }

        Ok(None)
    }

    /// Calls an RPC method on the PLC.
    ///
    /// Returns None if the PLC is not connected.
    pub fn invoke_rpc_method(&self, name: &str) -> Result<Option<()>> {
        let mut plc_connection_state = self.state.lock().unwrap();

        if let Some(client) = plc_connection_state.client_mut() {
            client.invoke_rpc_method(name).map_err(|error| {
                eprintln!(
                    "PLC client error when invoking RPC method {}: {}",
                    name, error
                );

                plc_connection_state.handle_disconnect_error(&error);

                error
            })?;

            return Ok(Some(()));
        }

        Ok(None)
    }

    /// Calls an RPC method on the PLC with one parameter.
    ///
    /// Returns None if the PLC is not connected.
    pub fn invoke_rpc_method_with_param<P: PlcDataType>(
        &self,
        name: &str,
        param: P,
    ) -> Result<Option<()>> {
        let mut plc_connection_state = self.state.lock().unwrap();

        if let Some(client) = plc_connection_state.client_mut() {
            client
                .invoke_rpc_method_with_param(name, param)
                .map_err(|error| {
                    eprintln!(
                        "PLC client error when invoking RPC method {}: {}",
                        name, error
                    );

                    plc_connection_state.handle_disconnect_error(&error);

                    error
                })?;

            return Ok(Some(()));
        }

        Ok(None)
    }

    /// Calls an RPC method on the PLC with three parameters.
    ///
    /// Returns None if the PLC is not connected.
    pub fn invoke_rpc_method_with_three_params<
        P1: PlcDataType,
        P2: PlcDataType,
        P3: PlcDataType,
    >(
        &self,
        name: &str,
        param_1: P1,
        param_2: P2,
        param_3: P3,
    ) -> Result<Option<()>> {
        let mut plc_connection_state = self.state.lock().unwrap();

        if let Some(client) = plc_connection_state.client_mut() {
            client
                .invoke_rpc_method_with_three_params(name, param_1, param_2, param_3)
                .map_err(|error| {
                    eprintln!(
                        "PLC client error when invoking RPC method {}: {}",
                        name, error
                    );

                    plc_connection_state.handle_disconnect_error(&error);

                    error
                })?;

            return Ok(Some(()));
        }

        Ok(None)
    }

    /// Subscribes to a notification channel on the PLC, returning a handle to the channel.
    ///
    /// Returns None if the PLC is not connected.
    pub fn subscribe<T: PlcDataType>(&self, name: &str) -> Result<Option<u32>> {
        let mut plc_connection_state = self.state.lock().unwrap();

        if let Some(client) = plc_connection_state.client_mut() {
            let handle = client.subscribe::<T>(name).map_err(|error| {
                eprintln!(
                    "PLC client error when subscribing to notifications from {}: {}",
                    name, error
                );

                plc_connection_state.handle_disconnect_error(&error);

                error
            })?;

            return Ok(Some(handle));
        }

        Ok(None)
    }

    /// Gets a notification receiver that streams symbol data as it changes on the PLC.
    ///
    /// A symbol must first be subscribed using the subscribe function.
    pub fn notification_receiver(&self) -> Option<Receiver<ads::notif::Notification>> {
        let plc_connection_state = self.state.lock().unwrap();

        plc_connection_state
            .client()
            .map(|client| client.notification_receiver())
    }
}

enum PlcConnectionState {
    Connected(PlcClient),
    Disconnected,
}

impl Default for PlcConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl PlcConnectionState {
    fn connect(
        &mut self,
        ads_router_address: SocketAddr,
        plc_ams_address: AmsAddr,
        local_ams_address: Option<AmsAddr>,
        set_to_run_mode: bool,
    ) -> Result<()> {
        match self {
            PlcConnectionState::Connected(_) => {
                println!("Attempted to connect to PLC but it is already connected!")
            }
            PlcConnectionState::Disconnected => {
                let ams_source: ads::Source =
                    local_ams_address.map_or(ads::Source::Request, ads::Source::Addr);

                let mut timeouts = ads::Timeouts::new(Duration::from_millis(1000));
                timeouts.read = Some(Duration::from_millis(2000));

                let ads_client = Client::new(ads_router_address, timeouts, ams_source)?;

                let plc_client = PlcClient::new(ads_client, plc_ams_address);

                if !plc_client.is_run_mode()? && set_to_run_mode {
                    plc_client.set_to_run_mode()?;
                }

                if !plc_client.is_run_mode()? {
                    return Err(anyhow!("PLC not in run mode, stopping connection."));
                }

                *self = PlcConnectionState::Connected(plc_client);
            }
        }

        Ok(())
    }

    fn disconnect(&mut self) {
        match self {
            PlcConnectionState::Connected(plc_client) => {
                plc_client.unsubscribe_all();

                *self = PlcConnectionState::Disconnected;

                println!("PLC connection was dropped.");
            }
            PlcConnectionState::Disconnected => {
                // Already disconnected...
            }
        }
    }

    fn client(&self) -> Option<&PlcClient> {
        match self {
            PlcConnectionState::Connected(plc_client) => Some(plc_client),
            PlcConnectionState::Disconnected => None,
        }
    }

    fn client_mut(&mut self) -> Option<&mut PlcClient> {
        match self {
            PlcConnectionState::Connected(plc_client) => Some(plc_client),
            PlcConnectionState::Disconnected => None,
        }
    }

    fn handle_disconnect_error(&mut self, error: &ads::Error) {
        let should_disconnect = matches!(
            error,
            ads::Error::Io(_, _)
                | ads::Error::Ads(_, _, 0x006)
                | ads::Error::Reply(_, "unexpected invoke ID", _)
        );

        if should_disconnect {
            println!("PLC client error indicates we should disconnect...");

            self.disconnect();
        }
    }
}

pub fn parse_socket_address_from_env(
    net_ip_key: &str,
    port_key: &str,
) -> Result<impl ToSocketAddrs> {
    let ads_router_ip: String = std::env::var(net_ip_key)
        .context(format!("Environment variable {net_ip_key} not found."))?;

    let ads_router_port: u16 = std::env::var(port_key)
        .context(format!("Environment variable {port_key} not found."))?
        .parse()?;

    Ok((ads_router_ip, ads_router_port))
}

pub fn parse_ams_address_from_env(net_id_key: &str, port_key: &str) -> Result<AmsAddr> {
    let net_id_string = std::env::var(net_id_key)
        .context(format!("Environment variable {net_id_key} not found."))?;
    let port_string =
        std::env::var(port_key).context(format!("Environment variable {port_key} not found."))?;

    let net_id: [u8; 6] = net_id_string
        .split('.')
        .map(|e| e.parse::<u8>().unwrap())
        .collect::<Vec<u8>>()
        .as_slice()
        .try_into()?;

    let port: u16 = port_string.parse()?;

    Ok(AmsAddr::new(net_id.into(), port))
}
