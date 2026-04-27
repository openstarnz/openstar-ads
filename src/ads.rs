use ads_client as ads;
use bon::bon;
use crossbeam_channel::Receiver;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

use crate::{AdsConnection, AdsData, AdsError, AdsParams, SymbolTypeTree, SymbolTypeTreeError};

pub struct Plc {
    addr: String,
    port: u16,
    timeout: Option<Timeout>,
    retry_delay: Option<Duration>,
    set_to_run_mode: bool,
    connection: Arc<Mutex<AdsConnection>>,
}

// Note(mw): We need our own Timeout enum, only because
//   ads_client::AdsTimeout doesn't implement Clone.
#[derive(Debug, Clone)]
pub enum Timeout {
    /// Default timeout is 5 seconds
    DefaultTimeout,
    /// Custom timeout (in seconds)
    CustomTimeout(u64),
}

impl From<Timeout> for ads::AdsTimeout {
    fn from(value: Timeout) -> Self {
        match value {
            Timeout::DefaultTimeout => ads::AdsTimeout::DefaultTimeout,
            Timeout::CustomTimeout(timeout) => ads::AdsTimeout::CustomTimeout(timeout),
        }
    }
}

/// Provides a more user friendly wrapper to interact with the OpenStar PLC's.
#[bon]
impl Plc {
    #[builder]
    pub fn new(
        addr: String,
        port: u16,
        timeout: Option<Timeout>,
        retry_delay: Option<Duration>,
        set_to_run_mode: bool,
    ) -> Self {
        Self {
            addr,
            port,
            timeout,
            retry_delay,
            set_to_run_mode,
            connection: Default::default(),
        }
    }

    /// Loops to keep a PLC connected over ADS.
    pub async fn connection_loop(&self) {
        loop {
            if !self.is_connected() {
                let mut connection = self.connection.lock().await;
                if let Err(error) = connection
                    .connect(
                        &self.addr,
                        self.port,
                        self.timeout.clone().map(Into::into),
                        self.retry_delay,
                        self.set_to_run_mode,
                    )
                    .await
                {
                    println!("PLC connection failed, {}. Retrying in 2 seconds...", error);
                } else {
                    println!("PLC connection successful!");
                }
            }

            std::thread::sleep(Duration::from_secs(2));
        }
    }

    /// Disconnects the internal PLC client
    pub async fn disconnect(&self) {
        let mut connection = self.connection.lock().await;
        connection.disconnect();
    }

    pub fn is_connected(&self) -> bool {
        let connection = self.connection.lock().await;
        match *connection {
            AdsConnection::Connected(_) => true,
            AdsConnection::Disconnected => false,
        }
    }

    /// Gets a symbol type tree for a given symbol path.
    ///
    /// Returns None if the PLC is not connected.
    /// Returns any errors from the PLC
    pub async fn get_dynamic_type_tree(&self, name: &str) -> Result<Option<SymbolTypeTree>> {
        let mut connection = self.connection.lock().await;

        if let Some(client) = connection.client_mut() {
            let type_tree = client.get_dynamic_type_tree(name).map_err(|error| {
                println!(
                    "PLC client error when getting information for symbol {}: {}",
                    name, error
                );

                match &error {
                    AdsError::SymbolTypeTree(_symbol_type_tree_error) => connection.disconnect(),
                    error => connection.handle_disconnect_error(error),
                };

                error
            })?;

            return Ok(Some(type_tree));
        }

        Ok(None)
    }

    /// Subscribes to the given symbol using the symbol type tree and sends deserialised tree-like data back with the sender channel.
    ///
    /// Returns None if the PLC is not connected.
    /// Returns any errors from the PLC.
    pub async fn start_dynamic_symbol_receiver(
        &self,
        name: String,
        symbol_type_tree: SymbolTypeTree,
        sender_channel: broadcast::Sender<SymbolTree>,
    ) -> Result<Option<()>> {
        let mut notif_handle = None;
        {
            let mut connection = self.connection.lock().await;

            if let Some(client) = connection.client_mut() {
                notif_handle = Some(
                    client
                        .add_dynamic_symbol_notification(&name, &symbol_type_tree)
                        .map_err(|error| {
                            println!(
                                "PLC client error when getting information for symbol {}: {}",
                                name, error
                            );

                            connection.handle_disconnect_error(&error);

                            error
                        })?,
                );
            }
        }

        let Some(mut notif_receiver) = self.notification_receiver() else {
            return Ok(None);
        };

        let Some(notif_handle) = notif_handle else {
            return Ok(None);
        };

        // Runs the actual notification loop which filters for the correct symbol handle.
        loop {
            let resp =
                tokio::task::spawn_blocking(move || Self::recv_blocking(notif_receiver)).await.context("Tokio spawn blocking function failed while trying to receive from PLC channel.")?;
            notif_receiver = resp.0;
            let notif_result = resp.1;
            let notif = notif_result
                .context("Receive error while receiving from PLC notification channel.")?;
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
    }

    /// Subscribes to the given symbol using the symbol type tree and sends deserialised flattened map data back with the sender channel.
    /// The key to the map is the path of the symbol relative to the named symbol provided.
    ///
    /// Returns None if the PLC is not connected.
    /// Returns any errors from the PLC.
    pub async fn start_dynamic_symbol_map_receiver(
        &self,
        name: String,
        symbol_type_tree: SymbolTypeTree,
        sender_channel: broadcast::Sender<SymbolMap>,
    ) -> Result<Option<()>> {
        let mut notif_handle = None;
        {
            let mut connection = self.connection.lock().await;

            if let Some(client) = connection.client_mut() {
                notif_handle = Some(
                    client
                        .add_dynamic_symbol_notification(&name, &symbol_type_tree)
                        .map_err(|error| {
                            println!(
                                "PLC client error when getting information for symbol {}: {}",
                                name, error
                            );

                            connection.handle_disconnect_error(&error);

                            error
                        })?,
                );
            }
        }

        let Some(mut notif_receiver) = self.notification_receiver() else {
            return Ok(None);
        };

        let Some(notif_handle) = notif_handle else {
            return Ok(None);
        };

        let symbol_type_map = SymbolTypeMap::from_tree(symbol_type_tree, 0, "".to_string());
        // Runs the actual notification loop which filters for the correct symbol handle.
        loop {
            let resp =
                tokio::task::spawn_blocking(move || Self::recv_blocking(notif_receiver)).await.context("Tokio spawn blocking function failed while trying to receive from PLC channel.")?;
            notif_receiver = resp.0;
            let notif_result = resp.1;
            let notif = notif_result
                .context("Receive error while receiving from PLC notification channel.")?;
            for sample in notif.samples() {
                if sample.handle == notif_handle {
                    let symbol_tree: SymbolMap =
                        SymbolMap::from_bytes(&symbol_type_map, sample.data);
                    // If there is an error sending it means that all receivers are gone and therefore this thread has successfully ended.
                    if let Err(_err) = sender_channel.send(symbol_tree) {
                        return Ok(Some(()));
                    };
                }
            }
        }
    }

    // Fully takes ownership of the receiver so that this can be run within a spawn_blocking enclosure.
    // Returns out the channel again with the result so that the caller can ensure the channel can be used again by awaiting the response.
    fn recv_blocking<T>(channel: Receiver<T>) -> (Receiver<T>, Result<T>) {
        let result = channel.recv_timeout(std::time::Duration::from_secs(5));
        let value = match result {
            Ok(value) => anyhow::Result::Ok(value),
            Err(err) => match err {
                crossbeam_channel::RecvTimeoutError::Timeout => {
                    anyhow::Result::Err(anyhow!("Message took more than 5 seconds to receive."))
                }
                crossbeam_channel::RecvTimeoutError::Disconnected => {
                    anyhow::Result::Err(anyhow!("PLC channel closed."))
                }
            },
        };
        (channel, value)
    }

    /// Read a symbol from the PLC.
    ///
    /// Returns None if the PLC is not connected.
    pub async fn read_symbol<T: AdsData>(&self, name: &str) -> Result<Option<T>> {
        let mut connection = self.connection.lock().await;

        if let Some(client) = connection.client_mut() {
            let value = client.read_symbol(name).map_err(|error| {
                println!("PLC client error when reading symbol {}: {}", name, error);

                connection.handle_disconnect_error(&error);

                error
            })?;

            return Ok(Some(value));
        }

        Ok(None)
    }

    /// Calls an RPC method on the PLC that returns a value.
    ///
    /// Returns None if the PLC is not connected.
    pub async fn fetch_from_rpc_method<T: AdsData>(&self, name: &str) -> Result<Option<T>> {
        let mut connection = self.connection.lock().await;

        if let Some(client) = connection.client_mut() {
            let value = client.fetch_from_rpc_method(name).map_err(|error| {
                eprintln!(
                    "PLC client error when invoking RPC method {}: {}",
                    name, error
                );

                connection.handle_disconnect_error(&error);

                error
            })?;

            return Ok(Some(value));
        }

        Ok(None)
    }

    /// Calls an RPC method on the PLC.
    ///
    /// Returns None if the PLC is not connected.
    pub async fn invoke_rpc_method(&self, name: &str) -> Result<Option<()>> {
        let mut connection = self.connection.lock().await;

        if let Some(client) = connection.client_mut() {
            client.invoke_rpc_method(name).map_err(|error| {
                eprintln!(
                    "PLC client error when invoking RPC method {}: {}",
                    name, error
                );

                connection.handle_disconnect_error(&error);

                error
            })?;

            return Ok(Some(()));
        }

        Ok(None)
    }

    /// Calls an RPC method on the PLC with one parameter.
    ///
    /// Returns None if the PLC is not connected.
    pub fn invoke_rpc_method_with_param<P: AdsData>(
        &self,
        name: &str,
        param: P,
    ) -> Result<Option<()>> {
        let mut connection = self.connection.lock().await;

        if let Some(client) = connection.client_mut() {
            client
                .invoke_rpc_method_with_param(name, param)
                .map_err(|error| {
                    eprintln!(
                        "PLC client error when invoking RPC method {}: {}",
                        name, error
                    );

                    connection.handle_disconnect_error(&error);

                    error
                })?;

            return Ok(Some(()));
        }

        Ok(None)
    }

    /// Calls an RPC method on the PLC with three parameters.
    ///
    /// Returns None if the PLC is not connected.
    pub fn invoke_rpc_method_with_three_params<P1: AdsData, P2: AdsData, P3: AdsData>(
        &self,
        name: &str,
        param_1: P1,
        param_2: P2,
        param_3: P3,
    ) -> Result<Option<()>> {
        let mut connection = self.connection.lock().await;

        if let Some(client) = connection.client_mut() {
            client
                .invoke_rpc_method_with_three_params(name, param_1, param_2, param_3)
                .map_err(|error| {
                    eprintln!(
                        "PLC client error when invoking RPC method {}: {}",
                        name, error
                    );

                    connection.handle_disconnect_error(&error);

                    error
                })?;

            return Ok(Some(()));
        }

        Ok(None)
    }

    /// Subscribes to a notification channel on the PLC, returning a handle to the channel.
    ///
    /// Returns None if the PLC is not connected.
    pub fn subscribe<T: AdsData>(&self, name: &str) -> Result<Option<u32>> {
        let mut connection = self.connection.lock().await;

        if let Some(client) = connection.client_mut() {
            let handle = client.subscribe::<T>(name).map_err(|error| {
                eprintln!(
                    "PLC client error when subscribing to notifications from {}: {}",
                    name, error
                );

                connection.handle_disconnect_error(&error);

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
        let connection = self.connection.lock().await;

        connection
            .client()
            .map(|client| client.notification_receiver())
    }
}
